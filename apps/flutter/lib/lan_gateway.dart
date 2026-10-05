import 'dart:io';

import 'package:flutter/services.dart';

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
    this.live = false,
    this.control = false,
    this.active = false,
  });
  final bool listening;
  final String address;
  final int requestId;
  final String peerFingerprint;
  final String deviceName;
  final bool live;
  final bool control;
  final bool active;
}

class LanFrame {
  const LanFrame({
    this.frameId = 0,
    this.width = 0,
    this.height = 0,
    this.closed = false,
    this.error = '',
  });
  final int frameId;
  final int width;
  final int height;
  final bool closed;
  final String error;
}

class LanInput {
  const LanInput(
    this.kind, {
    this.x = 0,
    this.y = 0,
    this.code = 0,
    this.pressed = false,
    this.delta = 0,
  });
  final int kind;
  final double x;
  final double y;
  final int code;
  final bool pressed;
  final int delta;
}

abstract interface class LanGateway {
  bool get canHost;
  Future<List<LocalLanAddress>> localAddresses();
  Future<LanHostState> startHost(String address);
  Future<LanHostState> hostStatus();
  Future<void> stopHost();
  Future<void> respond(int requestId, bool approved);
  Future<Uint8List> fetch(String address, String hostFingerprint);
  Future<void> cancel();
  Future<int> startLive(String address, String hostFingerprint);
  Future<LanFrame> pollLive(int sessionId);
  Future<void> sendInput(int sessionId, LanInput input);
  Future<void> stopLive(int sessionId);
  Future<int> createTexture(int sessionId);
  Future<void> disposeTexture(int textureId);
}

class NativeLanGateway implements LanGateway {
  NativeLanGateway({required this.canHost});
  static const videoChannel = MethodChannel('beodesk/video');
  @override
  Future<int> createTexture(int sessionId) async =>
      (await videoChannel.invokeMethod<int>('create', sessionId))!;
  @override
  Future<void> disposeTexture(int textureId) =>
      videoChannel.invokeMethod<void>('dispose', textureId);

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
    live: status.live,
    control: status.control,
    active: status.active,
  );
  @override
  Future<LanHostState> startHost(String address) async =>
      _state(await native.startSnapshotHost(address: address));
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
  @override
  Future<int> startLive(String address, String fingerprint) {
    if (!Platform.isLinux) {
      return Future.error(
        UnsupportedError(
          'Video trực tiếp H.264 hiện hỗ trợ Linux. Bạn vẫn có thể nhận một ảnh màn hình.',
        ),
      );
    }
    return native.startLiveSession(
      address: address,
      hostFingerprint: fingerprint,
    );
  }

  @override
  Future<LanFrame> pollLive(int sessionId) async {
    final frame = await native.pollLiveFrame(sessionId: sessionId);
    return LanFrame(
      frameId: frame.frameId,
      width: frame.width,
      height: frame.height,
      closed: frame.closed,
      error: frame.error,
    );
  }

  @override
  Future<void> sendInput(int sessionId, LanInput input) => native.sendLiveInput(
    sessionId: sessionId,
    kind: input.kind,
    x: input.x,
    y: input.y,
    code: input.code,
    pressed: input.pressed,
    delta: input.delta,
  );
  @override
  Future<void> stopLive(int sessionId) =>
      native.stopLiveSession(sessionId: sessionId);
}
