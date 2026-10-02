import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'connection_details.dart';
import 'lan_gateway.dart';

class ShareHostDialog extends StatefulWidget {
  const ShareHostDialog({
    super.key,
    required this.gateway,
    required this.localFingerprint,
    this.autoDetect = false,
  });
  final LanGateway gateway;
  final String localFingerprint;
  final bool autoDetect;

  @override
  State<ShareHostDialog> createState() => _ShareHostDialogState();
}

class _ShareHostDialogState extends State<ShareHostDialog> {
  final _bind = TextEditingController();
  final _peer = TextEditingController();
  bool _detecting = false;
  String? _lookupError;
  String? _clipboardNotice;
  bool _clipboardFailed = false;

  @override
  void initState() {
    super.initState();
    if (widget.autoDetect) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _detectAddress();
      });
    }
  }

  @override
  void dispose() {
    _bind.dispose();
    _peer.dispose();
    super.dispose();
  }

  Future<void> _detectAddress() async {
    setState(() {
      _detecting = true;
      _lookupError = null;
    });
    try {
      final addresses = await widget.gateway.localAddresses();
      if (!mounted) return;
      setState(() => _detecting = false);
      if (addresses.isEmpty) {
        setState(
          () => _lookupError = 'Không tìm thấy IP mạng. Hãy kết nối Wi-Fi/LAN hoặc nhập địa chỉ thủ công.',
        );
        return;
      }
      final selected = addresses.length == 1
          ? addresses.single
          : await showDialog<LocalLanAddress>(
              context: context,
              builder: (context) => SimpleDialog(
                title: const Text('Chọn IP máy này'),
                children: [
                  for (final address in addresses)
                    SimpleDialogOption(
                      onPressed: () => Navigator.pop(context, address),
                      child: ListTile(
                        leading: const Icon(Icons.lan_outlined),
                        title: Text(address.ip),
                        subtitle: Text(address.interfaceName),
                      ),
                    ),
                ],
              ),
            );
      if (!mounted || selected == null) return;
      final match = RegExp(r':([0-9]+)$').firstMatch(_bind.text.trim());
      final previousPort = int.tryParse(match?.group(1) ?? '');
      final port =
          previousPort != null && previousPort > 0 && previousPort <= 65535
          ? previousPort
          : 4433;
      _bind.text = '${selected.ip}:$port';
    } catch (_) {
      if (mounted) {
        setState(
          () => _lookupError = 'Không lấy được IP máy này. Bạn vẫn có thể nhập địa chỉ thủ công.',
        );
      }
    } finally {
      if (mounted) setState(() => _detecting = false);
    }
  }

  void _notice(String message, {bool failed = false}) {
    if (!mounted) return;
    setState(() {
      _clipboardNotice = message;
      _clipboardFailed = failed;
    });
  }

  Future<void> _pasteFingerprint() async {
    try {
      final clipboard = await Clipboard.getData(Clipboard.kTextPlain);
      final details = LanConnectionDetails.parse(clipboard?.text ?? '');
      if (!mounted) return;
      _peer.text = details.fingerprint;
      _notice('Đã dán dấu vân tay máy xem.');
    } catch (_) {
      _notice(
        'Clipboard cần chứa dấu vân tay đầy đủ hoặc thông tin được sao chép từ BeoDesk.',
        failed: true,
      );
    }
  }

  Future<void> _copyConnectionDetails() async {
    try {
      if (_bind.text.trim().isEmpty) await _detectAddress();
      if (!mounted || _bind.text.trim().isEmpty) return;
      final details = LanConnectionDetails(
        address: _bind.text,
        fingerprint: widget.localFingerprint,
      );
      await Clipboard.setData(ClipboardData(text: details.encode()));
      _notice('Đã sao chép IP + dấu vân tay máy này.');
    } catch (_) {
      _notice(
        'Không sao chép được. Hãy kiểm tra IP:cổng của máy này.',
        failed: true,
      );
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Bật chia sẻ ảnh màn hình'),
    content: SizedBox(
      width: 440,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              'Lấy IP máy này. Trên máy sẽ xem màn hình, sao chép dấu vân tay ở thẻ Máy của bạn '
              'rồi gửi sang đây để dán vào ô Dấu vân tay của máy xem. Mỗi ảnh vẫn cần bạn xác nhận.',
            ),
            const SizedBox(height: 20),
            TextField(
              controller: _bind,
              decoration: const InputDecoration(
                labelText: 'IP máy này và cổng',
                hintText: '192.168.1.20:4433',
              ),
            ),
            const SizedBox(height: 8),
            OutlinedButton.icon(
              key: const Key('detect-host-address'),
              onPressed: _detecting ? null : _detectAddress,
              icon: _detecting
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.my_location_outlined),
              label: Text(_detecting ? 'Đang lấy IP…' : 'Lấy IP + cổng'),
            ),
            if (_lookupError != null) ...[
              const SizedBox(height: 8),
              Text(
                _lookupError!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: 16),
            const Text(
              'Dấu vân tay máy này',
              style: TextStyle(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            SelectableText(
              widget.localFingerprint,
              style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
            ),
            const SizedBox(height: 8),
            OutlinedButton.icon(
              key: const Key('copy-local-details'),
              onPressed: _detecting ? null : _copyConnectionDetails,
              icon: const Icon(Icons.copy_outlined),
              label: const Text('Sao chép IP + vân tay'),
            ),
            const SizedBox(height: 20),
            TextField(
              controller: _peer,
              minLines: 2,
              maxLines: 3,
              decoration: const InputDecoration(
                labelText: 'Dấu vân tay của máy xem',
                helperText:
                    'Lấy từ thẻ Máy của bạn trên thiết bị sẽ xem màn hình.',
                helperMaxLines: 3,
              ),
            ),
            const SizedBox(height: 8),
            OutlinedButton.icon(
              key: const Key('paste-viewer-fingerprint'),
              onPressed: _pasteFingerprint,
              icon: const Icon(Icons.content_paste_outlined),
              label: const Text('Dán vân tay máy xem'),
            ),
            if (_clipboardNotice != null) ...[
              const SizedBox(height: 8),
              Text(
                _clipboardNotice!,
                style: TextStyle(
                  color: _clipboardFailed
                      ? Theme.of(context).colorScheme.error
                      : null,
                ),
              ),
            ],
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Hủy'),
      ),
      FilledButton(
        onPressed: _detecting
            ? null
            : () => Navigator.pop(context, (
                _bind.text.trim(),
                _peer.text.trim(),
              )),
        child: const Text('Bật chia sẻ'),
      ),
    ],
  );
}
