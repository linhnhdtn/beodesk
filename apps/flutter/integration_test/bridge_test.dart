import 'package:beodesk/app.dart';
import 'package:beodesk/engine_gateway.dart';
import 'package:beodesk/src/rust/api/app.dart' as native;
import 'package:beodesk/src/rust/frb_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

class _IntegrationGateway extends EngineGateway {
  @override
  Future<DeviceSnapshot> initialize() async {
    final info = native.engineInfo();
    return DeviceSnapshot(
      version: info.version,
      platform: info.platform,
      architecture: info.architecture,
    );
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => RustLib.init());
  testWidgets('loads bundled Rust engine and renders honest connection state', (
    tester,
  ) async {
    final info = native.engineInfo();
    expect(info.protocolMajor, 1);
    expect(info.canConnect, isTrue);
    expect(info.platform, isNotEmpty);
    await tester.pumpWidget(BeoDeskApp(gateway: _IntegrationGateway()));
    await tester.pumpAndSettle();
    expect(find.text('BeoDesk'), findsOneWidget);
    expect(find.textContaining('chưa khả dụng'), findsOneWidget);
  });
}
