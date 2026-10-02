import 'dart:io';
import 'dart:typed_data';

import 'src/rust/api/lan.dart' as native;

class LocalLanAddress {
  const LocalLanAddress({required this.ip, required this.interfaceName});
  final String ip;
  final String interfaceName;
}

class LanHostState {
  const LanHostState({
    required this.listening,
    this.address = '',
    this.requestId = 0,
    this.peerFingerprint = '',
    this.deviceName = '',
  });
  final bool listening;
  final String address;
  final int requestId;
  final String peerFingerprint;
  final String deviceName;
}

abstract interface class LanGateway {
  bool get canHost;
  Future<List<LocalLanAddress>> localAddresses();
  Future<LanHostState> startHost(String address, String peerFingerprint);
  Future<LanHostState> hostStatus();
  Future<void> stopHost();
  Future<void> respond(int requestId, bool approved);
  Future<Uint8List> fetch(String address, String hostFingerprint);
  Future<void> cancel();
}

class NativeLanGateway implements LanGateway {
  NativeLanGateway({required this.canHost});
  @override
  final bool canHost;

  @override
  Future<List<LocalLanAddress>> localAddresses() async {
    final interfaces = await NetworkInterface.list(
      includeLoopback: false,
      includeLinkLocal: false,
      type: InternetAddressType.IPv4,
    );
    final addresses = <LocalLanAddress>[];
    final seen = <String>{};
    for (final interface in interfaces) {
      for (final address in interface.addresses) {
        if (!address.isLoopback && seen.add(address.address)) {
          addresses.add(
            LocalLanAddress(ip: address.address, interfaceName: interface.name),
          );
        }
      }
    }
    // Show usual Ethernet/Wi-Fi interfaces before container and VPN adapters.
    int rank(LocalLanAddress address) =>
        RegExp(
          r'^(docker|br-|veth|virbr|tun|tap|wg|tailscale)|vethernet',
          caseSensitive: false,
        ).hasMatch(address.interfaceName)
        ? 1
        : 0;
    addresses.sort((a, b) {
      final priority = rank(a).compareTo(rank(b));
      return priority != 0 ? priority : a.ip.compareTo(b.ip);
    });
    return addresses;
  }

  LanHostState _state(native.HostStatus status) => LanHostState(
    listening: status.listening,
    address: status.address,
    requestId: status.requestId,
    peerFingerprint: status.peerFingerprint,
    deviceName: status.deviceName,
  );
  @override
  Future<LanHostState> startHost(String address, String fingerprint) async =>
      _state(
        await native.startSnapshotHost(
          address: address,
          peerFingerprint: fingerprint,
        ),
      );
  @override
  Future<LanHostState> hostStatus() async =>
      _state(await native.snapshotHostStatus());
  @override
  Future<void> stopHost() => native.stopSnapshotHost();
  @override
  Future<void> respond(int requestId, bool approved) =>
      native.respondToViewRequest(requestId: requestId, approved: approved);
  @override
  Future<Uint8List> fetch(String address, String fingerprint) =>
      native.fetchSnapshot(address: address, hostFingerprint: fingerprint);
  @override
  Future<void> cancel() => native.cancelSnapshotRequest();
}
