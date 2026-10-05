import 'dart:async';
import 'dart:collection';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'lan_gateway.dart';

class LiveSession extends StatefulWidget {
  const LiveSession({
    super.key,
    required this.gateway,
    required this.sessionId,
  });
  final LanGateway gateway;
  final int sessionId;

  @override
  State<LiveSession> createState() => _LiveSessionState();
}

class _LiveSessionState extends State<LiveSession> with WidgetsBindingObserver {
  final _focus = FocusNode();
  final _queue = Queue<LanInput>();
  final _keys = <int>{};
  late final Timer _pollTimer;
  late final Timer _motionTimer;
  int? _textureId;
  int _width = 0;
  int _height = 0;
  LanInput? _motion;
  bool _polling = false;
  bool _sending = false;
  bool _ended = false;
  bool _control = true;
  int _buttons = 0;
  String _error = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _pollTimer = Timer.periodic(
      const Duration(milliseconds: 100),
      (_) => _poll(),
    );
    _motionTimer = Timer.periodic(
      const Duration(milliseconds: 16),
      (_) => _drain(),
    );
    unawaited(_openTexture());
  }

  Future<void> _openTexture() async {
    try {
      final id = await widget.gateway.createTexture(widget.sessionId);
      if (!mounted || _ended) {
        await widget.gateway.disposeTexture(id);
        return;
      }
      setState(() => _textureId = id);
      await _poll();
    } catch (error) {
      _end('Không mở được video: $error');
    }
  }

  void _disposeTexture() {
    final id = _textureId;
    _textureId = null;
    if (id != null) {
      // Remove the Texture widget before releasing its native registration.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(widget.gateway.disposeTexture(id).catchError((Object _) {}));
      });
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) _release();
  }

  void _release() {
    _keys.clear();
    _buttons = 0;
    _motion = null;
    _queue.clear();
    if (!_ended) _enqueue(const LanInput(4));
  }

  Future<void> _poll() async {
    if (_polling || _ended) return;
    _polling = true;
    try {
      final frame = await widget.gateway.pollLive(widget.sessionId);
      if (!mounted || _ended) return;
      if (frame.closed) {
        _end(
          frame.error.isEmpty
              ? 'Máy chia sẻ đã kết thúc phiên.'
              : 'Kết nối đã ngắt: ${frame.error}',
        );
        return;
      }
      if (frame.width > 0 &&
          frame.height > 0 &&
          (_width != frame.width || _height != frame.height)) {
        setState(() {
          _width = frame.width;
          _height = frame.height;
        });
      }
    } catch (error) {
      _end('Không tiếp tục được phiên: $error');
    } finally {
      _polling = false;
    }
  }

  void _end(String error) {
    if (_ended) return;
    _ended = true;
    _disposeTexture();
    _queue.clear();
    _motion = null;
    _pollTimer.cancel();
    _motionTimer.cancel();
    unawaited(
      widget.gateway.stopLive(widget.sessionId).catchError((Object _) {}),
    );
    if (mounted) setState(() => _error = error);
  }

  void _enqueue(LanInput input) {
    if (_ended) return;
    if (_queue.length >= 128) {
      _end('Thao tác bị chậm. Hãy kết nối lại.');
      return;
    }
    // A click includes its own coordinates; old hover motion must not follow it.
    if (input.kind == 1) _motion = null;
    _queue.add(input);
    unawaited(_drain());
  }

  Future<void> _drain() async {
    if (_sending || _ended) return;
    _sending = true;
    try {
      while (!_ended && (_queue.isNotEmpty || _motion != null)) {
        final LanInput input;
        if (_queue.isNotEmpty) {
          input = _queue.removeFirst();
        } else {
          input = _motion!;
          _motion = null;
        }
        // Await every call: key/button up must never overtake down at the bridge.
        await widget.gateway.sendInput(widget.sessionId, input);
      }
    } catch (error) {
      _end('Không gửi được thao tác: $error');
    } finally {
      _sending = false;
    }
  }

  bool get _acceptInput =>
      _control && !_ended && _textureId != null && _width > 0;

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (!_acceptInput || !_focus.hasFocus) return KeyEventResult.ignored;
    // Local escape shortcut also works when the host has a held modifier.
    if (event.logicalKey == LogicalKeyboardKey.escape &&
        HardwareKeyboard.instance.isControlPressed &&
        HardwareKeyboard.instance.isAltPressed) {
      _release();
      _focus.unfocus();
      return KeyEventResult.handled;
    }
    final usage = event.physicalKey.usbHidUsage;
    if (usage >> 16 != 7) return KeyEventResult.ignored;
    final code = usage & 0xffff;
    if (code < 4 || code > 231) return KeyEventResult.ignored;
    // The host OS repeats held keys. Forwarding Flutter repeats would double it.
    if (event is KeyRepeatEvent) return KeyEventResult.handled;
    if (event is KeyDownEvent && _keys.add(code)) {
      _enqueue(LanInput(2, code: code, pressed: true));
    } else if (event is KeyUpEvent && _keys.remove(code)) {
      _enqueue(LanInput(2, code: code));
    }
    return KeyEventResult.handled;
  }

  Offset _point(PointerEvent event, Size size) => Offset(
    (event.localPosition.dx / size.width).clamp(0.0, 1.0),
    (event.localPosition.dy / size.height).clamp(0.0, 1.0),
  );

  void _pointer(PointerEvent event, Size size, {bool release = false}) {
    if (!_acceptInput) return;
    final point = _point(event, size);
    final next = release ? 0 : event.buttons;
    final changed = _buttons ^ next;
    const mapping = {
      kPrimaryMouseButton: 1,
      kMiddleMouseButton: 2,
      kSecondaryMouseButton: 3,
    };
    for (final entry in mapping.entries) {
      if (changed & entry.key != 0) {
        _enqueue(
          LanInput(
            1,
            x: point.dx,
            y: point.dy,
            code: entry.value,
            pressed: next & entry.key != 0,
          ),
        );
      }
    }
    _buttons = next;
    if (changed == 0) _motion = LanInput(0, x: point.dx, y: point.dy);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _ended = true;
    _disposeTexture();
    _pollTimer.cancel();
    _motionTimer.cancel();
    _focus.dispose();
    unawaited(
      widget.gateway.stopLive(widget.sessionId).catchError((Object _) {}),
    );
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => PopScope<void>(
    onPopInvokedWithResult: (didPop, _) {
      if (didPop) _end('');
    },
    child: Scaffold(
      backgroundColor: const Color(0xff101820),
      appBar: AppBar(
        title: Text(_ended ? 'Phiên đã kết thúc' : 'Màn hình trực tiếp'),
        leading: CloseButton(onPressed: () => Navigator.pop(context)),
        actions: [
          TextButton.icon(
            onPressed: _ended
                ? null
                : () {
                    _release();
                    setState(() => _control = !_control);
                    if (!_control) _focus.unfocus();
                  },
            icon: Icon(
              _control ? Icons.mouse_outlined : Icons.visibility_outlined,
            ),
            label: Text(_control ? 'Điều khiển' : 'Chỉ xem'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Ngắt kết nối'),
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Text(
              _ended ? _error : 'Bấm vào màn hình để dùng bàn phím · Ctrl + Alt + Esc để nhả điều khiển',
              style: const TextStyle(color: Colors.white),
              textAlign: TextAlign.center,
              maxLines: 4,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          Expanded(
            child: _ended
                ? const Center(
                    child: Icon(Icons.link_off, color: Colors.white, size: 48),
                  )
                : _textureId == null || _width == 0
                ? const Center(child: CircularProgressIndicator())
                : LayoutBuilder(
                    builder: (context, constraints) {
                      final fitted = applyBoxFit(
                        BoxFit.contain,
                        Size(_width.toDouble(), _height.toDouble()),
                        constraints.biggest,
                      ).destination;
                      return Center(
                        child: SizedBox(
                          width: fitted.width,
                          height: fitted.height,
                          child: Focus(
                            focusNode: _focus,
                            onKeyEvent: _key,
                            onFocusChange: (focused) {
                              if (!focused) _release();
                            },
                            child: MouseRegion(
                              cursor: _acceptInput
                                  ? SystemMouseCursors.precise
                                  : SystemMouseCursors.basic,
                              child: Listener(
                                key: const Key('remote-screen'),
                                behavior: HitTestBehavior.opaque,
                                onPointerDown: (e) {
                                  if (_acceptInput) _focus.requestFocus();
                                  _pointer(e, fitted);
                                },
                                onPointerUp: (e) => _pointer(e, fitted),
                                onPointerMove: (e) => _pointer(e, fitted),
                                onPointerHover: (e) => _pointer(e, fitted),
                                onPointerCancel: (_) => _release(),
                                onPointerSignal: (event) {
                                  if (event is PointerScrollEvent &&
                                      _acceptInput &&
                                      event.scrollDelta.dy != 0) {
                                    final point = _point(event, fitted);
                                    _enqueue(
                                      LanInput(0, x: point.dx, y: point.dy),
                                    );
                                    _enqueue(
                                      LanInput(
                                        3,
                                        delta:
                                            (event.scrollDelta.dy / 20)
                                                .round()
                                                .clamp(-10, 10) *
                                            120,
                                      ),
                                    );
                                  }
                                },
                                child: Texture(
                                  textureId: _textureId!,
                                  filterQuality: FilterQuality.low,
                                ),
                              ),
                            ),
                          ),
                        ),
                      );
                    },
                  ),
          ),
        ],
      ),
    ),
  );
}
