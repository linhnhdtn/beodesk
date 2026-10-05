import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'lan_gateway.dart';
import 'connection_details.dart';
import 'share_host_dialog.dart';
import 'live_session.dart';

class LanPanel extends StatefulWidget {
  const LanPanel({
    super.key,
    required this.gateway,
    required this.localFingerprint,
  });
  final LanGateway gateway;
  final String localFingerprint;
  @override
  State<LanPanel> createState() => _LanPanelState();
}

class _LanPanelState extends State<LanPanel> {
  final _address = TextEditingController(text: '127.0.0.1:4433');
  final _fingerprint = TextEditingController();
  late final Timer _timer;
  bool _hosting = false;
  bool _fetching = false;
  bool _starting = false;
  bool _checking = false;
  bool _consentOpen = false;
  bool _hostActive = false;
  int? _liveId;
  String _listeningAddress = '';

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(milliseconds: 500), (_) => _poll());
  }

  @override
  void dispose() {
    _timer.cancel();
    _address.dispose();
    _fingerprint.dispose();
    if (_hosting) {
      unawaited(widget.gateway.stopHost().catchError((Object _) {}));
    }
    if (_fetching) unawaited(widget.gateway.cancel().catchError((Object _) {}));
    if (_liveId != null) {
      unawaited(widget.gateway.stopLive(_liveId!).catchError((Object _) {}));
    }
    super.dispose();
  }

  void _error(Object error, {bool receiving = false}) {
    if (!mounted) return;
    final details = error is AnyhowException ? error.message : error.toString();
    final lower = details.toLowerCase();
    final String message;
    if (lower.contains('fingerprint does not match')) {
      // A remote TLS rejection means the host rejected our viewer identity.
      // A local failure while authenticating the host rejects its certificate.
      if (receiving && lower.contains('aborted by peer')) {
        message = 'Máy chia sẻ đang dùng cách kết nối cũ. Cập nhật BeoDesk trên máy chia sẻ rồi bật lại chia sẻ.';
      } else if (receiving && lower.contains('authenticating lan host at ')) {
        message = 'Dấu vân tay máy chia sẻ không khớp. Sao chép lại IP + vân tay từ máy chia sẻ và đối chiếu đầy đủ.';
      } else {
        message = 'Dấu vân tay xác thực không khớp. Hãy đối chiếu với dấu vân tay trên máy chia sẻ.';
      }
    } else if (lower.contains('connection lost') ||
        lower.contains('transport error') ||
        lower.contains('cryptographic handshake')) {
      message = 'Kết nối bị ngắt. Kiểm tra IP và vân tay máy chia sẻ; mở Chi tiết để xem nguyên nhân.';
    } else if (lower.contains('timed out') ||
        lower.contains('connection refused')) {
      message = 'Không nhận được phản hồi. Kiểm tra IP, cổng, chia sẻ đang bật và UDP/firewall.';
    } else {
      message = details.split('\n').first;
    }
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text('Không thực hiện được: $message'),
        duration: const Duration(seconds: 12),
        action: SnackBarAction(
          label: 'Chi tiết',
          onPressed: () => showDialog<void>(
            context: context,
            builder: (context) => AlertDialog(
              title: const Text('Chi tiết lỗi'),
              content: SingleChildScrollView(child: SelectableText(details)),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Đóng'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _poll() async {
    if (!_hosting || _checking || _consentOpen) return;
    _checking = true;
    try {
      final state = await widget.gateway.hostStatus();
      if (!mounted) return;
      if (!state.listening) {
        setState(() => _hosting = false);
        return;
      }
      if (_hostActive != state.active) {
        setState(() => _hostActive = state.active);
      }
      if (state.requestId == 0) return;
      _consentOpen = true;
      final approved = await showDialog<bool>(
        context: context,
        barrierDismissible: false,
        builder: (context) =>
            _ConsentDialog(gateway: widget.gateway, request: state),
      );
      if (!mounted || approved == null) return;
      await widget.gateway.respond(state.requestId, approved);
    } catch (error) {
      _error(error);
    } finally {
      _checking = false;
      _consentOpen = false;
    }
  }

  Future<void> _startHost({bool detectAddress = true}) async {
    final address = await showDialog<String>(
      context: context,
      builder: (context) => ShareHostDialog(
        gateway: widget.gateway,
        localFingerprint: widget.localFingerprint,
        autoDetect: detectAddress,
      ),
    );
    if (address == null || !mounted) return;
    setState(() => _starting = true);
    try {
      final state = await widget.gateway.startHost(address);
      if (!mounted) {
        await widget.gateway.stopHost();
        return;
      }
      setState(() {
        _hosting = state.listening;
        _listeningAddress = state.address;
      });
    } catch (error) {
      _error(error);
    } finally {
      if (mounted) setState(() => _starting = false);
    }
  }

  Future<void> _stopHost() async {
    try {
      await widget.gateway.stopHost();
      if (mounted) setState(() => _hosting = false);
    } catch (error) {
      _error(error);
    }
  }

  Future<void> _pasteConnectionDetails() async {
    try {
      final clipboard = await Clipboard.getData(Clipboard.kTextPlain);
      final details = LanConnectionDetails.parse(clipboard?.text ?? '');
      if (!mounted || _fetching) return;
      _fingerprint.text = details.fingerprint;
      if (details.address != null) _address.text = details.address!;
    } catch (error) {
      _error(error);
    }
  }

  Future<void> _copyHostDetails() async {
    try {
      await Clipboard.setData(
        ClipboardData(
          text: LanConnectionDetails(
            address: _listeningAddress,
            fingerprint: widget.localFingerprint,
          ).encode(),
        ),
      );
      if (!mounted) return;
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(
          const SnackBar(
            content: Text(
              'Đã sao chép IP + vân tay. Gửi cho máy muốn điều khiển.',
            ),
          ),
        );
    } catch (error) {
      _error(error);
    }
  }

  Future<void> _fetch() async {
    if (_address.text.trim().isEmpty || _fingerprint.text.trim().isEmpty) {
      _error('Cần địa chỉ IP và dấu vân tay đầy đủ của máy chia sẻ.');
      return;
    }
    setState(() {
      _fetching = true;
    });
    try {
      final image = await widget.gateway.fetch(
        LanConnectionDetails.normalizeAddress(_address.text),
        _fingerprint.text.trim(),
      );
      if (!mounted) return;
      setState(() => _fetching = false);
      await showDialog<void>(
        context: context,
        builder: (context) => Dialog.fullscreen(
          child: Scaffold(
            appBar: AppBar(
              title: const Text('Ảnh màn hình đã nhận'),
              leading: CloseButton(onPressed: () => Navigator.pop(context)),
            ),
            body: Column(
              children: [
                const Padding(
                  padding: EdgeInsets.all(16),
                  child: Text('Ảnh chụp tại thời điểm máy chia sẻ cho phép.'),
                ),
                Expanded(
                  child: InteractiveViewer(
                    child: Center(
                      child: Image.memory(
                        image,
                        fit: BoxFit.contain,
                        errorBuilder: (_, _, _) =>
                            const Text('Không hiển thị được ảnh.'),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    } catch (error) {
      _error(error, receiving: true);
    } finally {
      if (mounted) setState(() => _fetching = false);
    }
  }

  Future<void> _connectLive() async {
    if (_address.text.trim().isEmpty || _fingerprint.text.trim().isEmpty) {
      _error('Cần địa chỉ IP và dấu vân tay đầy đủ của máy chia sẻ.');
      return;
    }
    setState(() {
      _fetching = true;
    });
    int? id;
    try {
      id = await widget.gateway.startLive(
        LanConnectionDetails.normalizeAddress(_address.text),
        _fingerprint.text.trim(),
      );
      _liveId = id;
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        barrierDismissible: false,
        builder: (_) => Dialog.fullscreen(
          child: LiveSession(gateway: widget.gateway, sessionId: id!),
        ),
      );
    } catch (error) {
      _error(error, receiving: true);
    } finally {
      if (id != null) await widget.gateway.stopLive(id);
      _liveId = null;
      if (mounted) setState(() => _fetching = false);
    }
  }

  @override
  Widget build(BuildContext context) => Container(
    width: double.infinity,
    padding: const EdgeInsets.all(24),
    decoration: BoxDecoration(
      color: Colors.white,
      borderRadius: BorderRadius.circular(18),
      border: Border.all(color: const Color(0xffdfe6ec)),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Icon(Icons.devices_rounded, size: 32, color: Color(0xff087f8c)),
        const SizedBox(height: 16),
        Text(
          'Xem màn hình qua LAN',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 12),
        const Text(
          'Xem màn hình liên tục và dùng chuột, bàn phím sau khi máy chia sẻ cho phép.',
        ),
        const SizedBox(height: 20),
        TextField(
          controller: _address,
          enabled: !_fetching,
          decoration: const InputDecoration(
            labelText: 'IP máy chia sẻ và cổng',
            hintText: '192.168.1.20 (cổng mặc định 4433)',
          ),
        ),
        const SizedBox(height: 16),
        TextField(
          controller: _fingerprint,
          enabled: !_fetching,
          minLines: 2,
          maxLines: 3,
          decoration: const InputDecoration(
            labelText: 'Dấu vân tay của máy chia sẻ',
          ),
        ),
        const SizedBox(height: 8),
        OutlinedButton.icon(
          key: const Key('paste-remote-details'),
          onPressed: _fetching ? null : _pasteConnectionDetails,
          icon: const Icon(Icons.content_paste_outlined),
          label: const Text('Dán IP + vân tay'),
        ),
        const SizedBox(height: 16),
        SizedBox(
          width: double.infinity,
          child: FilledButton.icon(
            key: const Key('connect-live'),
            onPressed: _fetching ? null : _connectLive,
            icon: const Icon(Icons.connected_tv_outlined),
            label: const Text('Kết nối và điều khiển'),
          ),
        ),
        const SizedBox(height: 8),
        SizedBox(
          width: double.infinity,
          child: OutlinedButton.icon(
            onPressed: _fetching ? null : _fetch,
            icon: const Icon(Icons.image_outlined),
            label: const Text('Nhận ảnh màn hình'),
          ),
        ),
        if (_fetching) ...[
          const SizedBox(height: 12),
          const LinearProgressIndicator(),
          const SizedBox(height: 8),
          const Text('Đang kết nối hoặc chờ máy chia sẻ xác nhận…'),
          TextButton(
            onPressed: () async {
              try {
                await widget.gateway.cancel();
              } catch (error) {
                _error(error);
              }
            },
            child: const Text('Hủy yêu cầu'),
          ),
        ],
        const Divider(height: 40),
        if (_hosting) ...[
          Text(
            _hostActive
                ? 'Đang chia sẻ màn hình trong một phiên trực tiếp'
                : 'Đang chờ yêu cầu tại $_listeningAddress',
            style: TextStyle(
              color: _hostActive ? Colors.deepOrange : const Color(0xff087f8c),
              fontWeight: _hostActive ? FontWeight.bold : FontWeight.normal,
            ),
          ),
          const SizedBox(height: 12),
          const Text(
            'Gửi IP + vân tay cho máy muốn điều khiển. Bạn sẽ xác nhận từng yêu cầu kết nối.',
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            key: const Key('copy-host-details'),
            onPressed: _copyHostDetails,
            icon: const Icon(Icons.copy_outlined),
            label: const Text('Sao chép IP + vân tay'),
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            onPressed: _stopHost,
            icon: const Icon(Icons.stop_circle_outlined),
            label: const Text('Dừng chia sẻ'),
          ),
        ] else ...[
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton.icon(
                onPressed: widget.gateway.canHost && !_starting
                    ? _startHost
                    : null,
                icon: const Icon(Icons.screen_share_outlined),
                label: Text(
                  _starting ? 'Đang bật…' : 'Chia sẻ màn hình máy này',
                ),
              ),
              OutlinedButton.icon(
                key: const Key('detect-local-endpoint'),
                onPressed: widget.gateway.canHost && !_starting
                    ? () => _startHost(detectAddress: true)
                    : null,
                icon: const Icon(Icons.my_location_outlined),
                label: const Text('Lấy IP + cổng'),
              ),
            ],
          ),
          if (!widget.gateway.canHost)
            const Padding(
              padding: EdgeInsets.only(top: 8),
              child: Text('Chia sẻ hiện hỗ trợ Ubuntu GNOME trên X11.'),
            ),
        ],
      ],
    ),
  );
}

class _ConsentDialog extends StatefulWidget {
  const _ConsentDialog({required this.gateway, required this.request});
  final LanGateway gateway;
  final LanHostState request;
  @override
  State<_ConsentDialog> createState() => _ConsentDialogState();
}

class _ConsentDialogState extends State<_ConsentDialog> {
  late final Timer _timer;
  bool _checking = false;
  bool _closing = false;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(milliseconds: 500), (_) => _check());
  }

  Future<void> _check() async {
    if (_checking || _closing) return;
    _checking = true;
    try {
      final status = await widget.gateway.hostStatus();
      if (!status.listening || status.requestId != widget.request.requestId) {
        _close();
      }
    } catch (_) {
      _close();
    } finally {
      _checking = false;
    }
  }

  void _close([bool? approved]) {
    if (!mounted || _closing) return;
    _closing = true;
    // The dialog stays mounted during its reverse animation. Stop polling
    // before popping, and ignore any native status request already in flight.
    _timer.cancel();
    if (ModalRoute.of(context)?.isCurrent == true) {
      Navigator.pop(context, approved);
    }
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(
      widget.request.control
          ? 'Cho phép xem và điều khiển?'
          : 'Cho phép xem màn hình?',
    ),
    content: SingleChildScrollView(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            widget.request.live
                ? '${widget.request.deviceName} muốn xem màn hình liên tục${widget.request.control ? ' và điều khiển chuột, bàn phím' : ''} của máy này.'
                : '${widget.request.deviceName} muốn nhận một ảnh chụp màn hình của máy này.',
          ),
          const SizedBox(height: 16),
          const Text('Dấu vân tay máy đang yêu cầu:'),
          const SizedBox(height: 8),
          SelectableText(widget.request.peerFingerprint),
          const SizedBox(height: 16),
          Text(
            widget.request.live
                ? 'Yêu cầu hết hạn sau 60 giây. Bạn có thể ngắt phiên bất cứ lúc nào bằng Dừng chia sẻ.'
                : 'Yêu cầu hết hạn sau 60 giây. Không cấp quyền điều khiển chuột hoặc bàn phím.',
          ),
        ],
      ),
    ),
    actions: [
      TextButton(onPressed: () => _close(false), child: const Text('Từ chối')),
      FilledButton(
        onPressed: () => _close(true),
        child: Text(
          widget.request.control
              ? 'Cho phép điều khiển'
              : widget.request.live
              ? 'Cho phép xem liên tục'
              : 'Cho phép một ảnh',
        ),
      ),
    ],
  );
}
