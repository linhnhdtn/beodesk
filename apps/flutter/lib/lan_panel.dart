import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'lan_gateway.dart';
import 'connection_details.dart';
import 'share_host_dialog.dart';

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
  bool _viewerRejected = false;
  String _listeningAddress = '';
  String _trustedViewerFingerprint = '';

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
        setState(() => _viewerRejected = true);
        message = 'Máy chia sẻ từ chối dấu vân tay của máy này. Cập nhật vân tay máy xem rồi bật lại chia sẻ.';
      } else if (receiving && lower.contains('authenticating lan host at ')) {
        message = 'Dấu vân tay máy chia sẻ không khớp. Sao chép lại IP + vân tay từ máy chia sẻ và đối chiếu đầy đủ.';
      } else {
        message = 'Dấu vân tay xác thực không khớp. Hãy đối chiếu đầy đủ ở cả hai máy.';
      }
    } else if (lower.contains('connection lost') ||
        lower.contains('transport error') ||
        lower.contains('cryptographic handshake')) {
      message = 'Kết nối bị ngắt. Kiểm tra dấu vân tay ở cả hai máy; mở Chi tiết để xem nguyên nhân.';
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

  Future<void> _startHost({bool detectAddress = false}) async {
    final values = await showDialog<(String, String)>(
      context: context,
      builder: (context) => ShareHostDialog(
        gateway: widget.gateway,
        localFingerprint: widget.localFingerprint,
        autoDetect: detectAddress,
      ),
    );
    if (values == null || !mounted) return;
    setState(() => _starting = true);
    try {
      final state = await widget.gateway.startHost(values.$1, values.$2);
      if (!mounted) {
        await widget.gateway.stopHost();
        return;
      }
      setState(() {
        _hosting = state.listening;
        _listeningAddress = state.address;
        _trustedViewerFingerprint = values.$2;
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

  Future<void> _copyViewerFingerprint() async {
    try {
      await Clipboard.setData(ClipboardData(text: widget.localFingerprint));
      if (!mounted) return;
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(
          const SnackBar(
            content: Text(
              'Đã sao chép dấu vân tay máy xem. Gửi cho máy chia sẻ để đối chiếu.',
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
      _viewerRejected = false;
    });
    try {
      final image = await widget.gateway.fetch(
        _address.text.trim(),
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
          'Prototype nhận một ảnh màn hình. Video liên tục và điều khiển chuột/phím sẽ được bổ sung ở bước sau.',
        ),
        const SizedBox(height: 20),
        TextField(
          controller: _address,
          enabled: !_fetching,
          decoration: const InputDecoration(
            labelText: 'IP máy chia sẻ và cổng',
            hintText: '192.168.1.20:4433',
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
            onPressed: _fetching ? null : _fetch,
            icon: const Icon(Icons.image_outlined),
            label: const Text('Nhận ảnh màn hình'),
          ),
        ),
        if (_viewerRejected) ...[
          const SizedBox(height: 16),
          const Text(
            'Cập nhật dấu vân tay ở máy chia sẻ',
            style: TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          const Text(
            'Sao chép dấu vân tay máy này bên dưới và gửi qua kênh tin cậy. '
            'Ở máy chia sẻ, chọn Dừng chia sẻ, mở lại Chia sẻ màn hình máy này, '
            'dán vào ô Dấu vân tay của máy xem và đối chiếu đầy đủ rồi Bật chia sẻ. '
            'Sau đó quay lại đây để nhận ảnh.',
          ),
          const SizedBox(height: 8),
          SelectableText(
            widget.localFingerprint,
            style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            key: const Key('copy-viewer-fingerprint'),
            onPressed: _copyViewerFingerprint,
            icon: const Icon(Icons.copy_outlined),
            label: const Text('Sao chép vân tay máy xem'),
          ),
        ],
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
            'Đang chờ yêu cầu tại $_listeningAddress',
            style: const TextStyle(color: Color(0xff087f8c)),
          ),
          const SizedBox(height: 12),
          const Text('Dấu vân tay máy xem được phép kết nối:'),
          const SizedBox(height: 8),
          SelectableText(
            _trustedViewerFingerprint,
            key: const Key('trusted-viewer-fingerprint'),
            style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
          ),
          const SizedBox(height: 8),
          const Text(
            'Để đổi máy xem hoặc sửa dấu vân tay, dừng chia sẻ rồi bật lại.',
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

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(milliseconds: 500), (_) => _check());
  }

  Future<void> _check() async {
    if (_checking) return;
    _checking = true;
    try {
      final status = await widget.gateway.hostStatus();
      if (mounted &&
          (!status.listening || status.requestId != widget.request.requestId)) {
        Navigator.pop(context);
      }
    } catch (_) {
      if (mounted) Navigator.pop(context);
    } finally {
      _checking = false;
    }
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Cho phép xem màn hình?'),
    content: SingleChildScrollView(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${widget.request.deviceName} muốn nhận một ảnh chụp màn hình của máy này.',
          ),
          const SizedBox(height: 16),
          const Text('Thiết bị đã xác thực:'),
          const SizedBox(height: 8),
          SelectableText(widget.request.peerFingerprint),
          const SizedBox(height: 16),
          const Text(
            'Yêu cầu hết hạn sau 60 giây. Không cấp quyền điều khiển chuột hoặc bàn phím.',
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context, false),
        child: const Text('Từ chối'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, true),
        child: const Text('Cho phép một ảnh'),
      ),
    ],
  );
}
