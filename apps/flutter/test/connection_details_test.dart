import 'package:beodesk/connection_details.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('shared text preserves the complete address and fingerprint', () {
    final original = LanConnectionDetails(
      address: '172.16.1.126:4433',
      fingerprint: 'a' * 64,
    );
    final decoded = LanConnectionDetails.parse(original.encode());
    expect(decoded.address, '172.16.1.126:4433');
    expect(decoded.fingerprint.replaceAll(' ', ''), 'A' * 64);
  });

  test('paste accepts a raw pin or a CRLF packet with an IPv6 endpoint', () {
    final raw = LanConnectionDetails.parse('  ${'b' * 64}\n');
    expect(raw.address, isNull);
    expect(raw.fingerprint.replaceAll(' ', ''), 'B' * 64);
    final ipv6 = LanConnectionDetails.parse(
      'BeoDesk\r\nIP: [fd00::1]:5444\r\nDấu vân tay: ${'c' * 64}',
    );
    expect(ipv6.address, '[fd00::1]:5444');
  });

  test('incomplete fingerprints and unusable endpoints cannot be applied', () {
    for (final value in [
      '12345678',
      'g' * 64,
      'x' * 2049,
      'BeoDesk\nIP: host.example:4433\nDấu vân tay: ${'a' * 64}',
      'BeoDesk\nIP: 0.0.0.0:4433\nDấu vân tay: ${'a' * 64}',
      'BeoDesk\nIP: 192.168.1.2:0\nDấu vân tay: ${'a' * 64}',
      'BeoDesk\nIP: 192.168.1.2:4433\nDấu vân tay: 12345678',
    ]) {
      expect(() => LanConnectionDetails.parse(value), throwsFormatException);
    }
  });
}
