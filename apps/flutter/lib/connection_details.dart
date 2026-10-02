import 'dart:io';

class LanConnectionDetails {
  const LanConnectionDetails({required this.fingerprint, this.address});
  final String fingerprint;
  final String? address;

  static String normalizeFingerprint(String value) {
    final hex = value.replaceAll(RegExp(r'\s'), '').toUpperCase();
    if (!RegExp(r'^[0-9A-F]{64}$').hasMatch(hex)) {
      throw const FormatException('Dấu vân tay cần đủ 64 ký tự hex.');
    }
    return List.generate(
      8,
      (index) => hex.substring(index * 8, index * 8 + 8),
    ).join(' ');
  }

  static String normalizeAddress(String value) {
    final uri = Uri.tryParse('udp://${value.trim()}');
    final ip = uri == null ? null : InternetAddress.tryParse(uri.host);
    if (uri == null ||
        ip == null ||
        !uri.hasPort ||
        uri.port < 1 ||
        uri.port > 65535 ||
        uri.userInfo.isNotEmpty ||
        uri.path.isNotEmpty ||
        uri.hasQuery ||
        uri.hasFragment ||
        ip.rawAddress.every((byte) => byte == 0) ||
        (ip.type == InternetAddressType.IPv4 && ip.rawAddress.first >= 224) ||
        (ip.type == InternetAddressType.IPv6 && ip.rawAddress.first == 255)) {
      throw const FormatException(
        'Cần địa chỉ IP và cổng hợp lệ, ví dụ 192.168.1.20:4433.',
      );
    }
    return ip.type == InternetAddressType.IPv6
        ? '[${ip.address}]:${uri.port}'
        : '${ip.address}:${uri.port}';
  }

  static LanConnectionDetails parse(String value) {
    if (value.length > 2048 || value.trim().isEmpty) {
      throw const FormatException(
        'Clipboard chưa có thông tin BeoDesk hợp lệ.',
      );
    }
    final text = value.trim();
    final packet = RegExp(
      r'^BeoDesk\r?\nIP:\s*(\S+)\r?\nDấu vân tay:\s*([0-9a-fA-F\s]+)$',
    ).firstMatch(text);
    if (packet != null) {
      return LanConnectionDetails(
        address: normalizeAddress(packet.group(1)!),
        fingerprint: normalizeFingerprint(packet.group(2)!),
      );
    }
    return LanConnectionDetails(fingerprint: normalizeFingerprint(text));
  }

  String encode() =>
      'BeoDesk\nIP: ${normalizeAddress(address ?? '')}\nDấu vân tay: ${normalizeFingerprint(fingerprint)}';
}
