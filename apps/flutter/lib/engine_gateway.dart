import 'src/rust/api/app.dart' as native;
import 'src/rust/frb_generated.dart';
import 'lan_gateway.dart';

class DeviceSnapshot {
  const DeviceSnapshot({
    required this.version,
    required this.platform,
    required this.architecture,
    this.fingerprint,
    this.storage,
  });
  final String version;
  final String platform;
  final String architecture;
  final String? fingerprint;
  final String? storage;
}

abstract class EngineGateway {
  Future<DeviceSnapshot> initialize();
  LanGateway? get lan => null;
}

class NativeEngineGateway extends EngineGateway {
  bool _initialized = false;
  LanGateway? _lan;
  @override
  LanGateway? get lan => _lan;

  @override
  Future<DeviceSnapshot> initialize() async {
    if (!_initialized) {
      await RustLib.init();
      _initialized = true;
    }
    final info = native.engineInfo();
    _lan = NativeLanGateway(canHost: info.canHost);
    native.DeviceInfo? identity;
    try {
      identity = await native.initializeDevice();
    } catch (_) {
      // Never fall back to a new ephemeral identity when secure storage fails.
    }
    return DeviceSnapshot(
      version: info.version,
      platform: info.platform,
      architecture: info.architecture,
      fingerprint: identity?.fingerprint,
      storage: identity?.storageDescription,
    );
  }
}
