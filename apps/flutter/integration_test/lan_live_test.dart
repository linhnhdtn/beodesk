// Real bridge/QUIC/H.264/native texture integration. Run on a private Xvfb with an unlocked keyring.
import 'dart:async';
import 'dart:io';

import 'package:beodesk/app.dart';
import 'package:beodesk/engine_gateway.dart';
import 'package:beodesk/lan_gateway.dart';
import 'package:beodesk/src/rust/api/lan.dart' as lan;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

class _DelayedLan extends NativeLanGateway {
  _DelayedLan({required super.canHost});
  Completer<LanHostState>? heldStatus;
  bool statusRequested = false;
  @override
  Future<LanHostState> hostStatus() {
    if (heldStatus != null) {
      statusRequested = true;
      return heldStatus!.future;
    }
    return super.hostStatus();
  }
}

class _Gateway extends EngineGateway {
  _Gateway(this.device, this.lan);
  final DeviceSnapshot device;
  @override
  final _DelayedLan lan;
  @override
  Future<DeviceSnapshot> initialize() async => device;
}

Future<void> waitFor(WidgetTester tester, Finder finder) async {
  for (var i = 0; i < 200; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
  }
  fail('Timed out waiting for $finder');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'native live session survives a late consent status, streams frames and disconnects',
    (tester) async {
      final native = NativeEngineGateway();
      final device = await native.initialize();
      final gateway = _Gateway(
        device,
        _DelayedLan(canHost: native.lan!.canHost),
      );
      final socket = await RawDatagramSocket.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      final address = '127.0.0.1:${socket.port}';
      socket.close();
      addTearDown(() async {
        await lan.cancelSnapshotRequest();
        await lan.stopSnapshotHost();
      });
      await tester.pumpWidget(BeoDeskApp(gateway: gateway));
      await waitFor(tester, find.text('Chia sẻ màn hình máy này'));
      await tester.ensureVisible(find.text('Chia sẻ màn hình máy này'));
      await tester.tap(find.text('Chia sẻ màn hình máy này'));
      await tester.pumpAndSettle();
      if (find.text('Chọn IP máy này').evaluate().isNotEmpty) {
        final addresses = await gateway.lan.localAddresses();
        await tester.tap(find.text(addresses.first.ip));
        await tester.pumpAndSettle();
      }
      await tester.enterText(
        find.widgetWithText(TextField, 'IP máy này và cổng'),
        address,
      );
      expect(
        find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
        findsNothing,
      );
      await tester.tap(find.text('Bật chia sẻ'));
      await waitFor(tester, find.text('Dừng chia sẻ'));
      await tester.enterText(
        find.widgetWithText(TextField, 'IP máy chia sẻ và cổng'),
        address,
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
        device.fingerprint!,
      );
      await tester.ensureVisible(find.text('Kết nối và điều khiển'));
      await tester.tap(find.text('Kết nối và điều khiển'));
      await waitFor(tester, find.text('Cho phép xem và điều khiển?'));
      expect((await lan.snapshotHostStatus()).active, isFalse);
      expect((await lan.snapshotHostStatus()).control, isTrue);
      // Delay a real status call until approval has consumed the request,
      // while the closing consent dialog remains mounted for its animation.
      final delayed = Completer<LanHostState>();
      gateway.lan.heldStatus = delayed;
      await tester.pump(const Duration(milliseconds: 500));
      expect(gateway.lan.statusRequested, isTrue);
      await tester.tap(find.text('Cho phép điều khiển'));
      await tester.pump();
      for (
        var i = 0;
        i < 50 && (await lan.snapshotHostStatus()).requestId != 0;
        i++
      ) {
        await tester.pump(const Duration(milliseconds: 2));
      }
      expect((await lan.snapshotHostStatus()).requestId, 0);
      gateway.lan.heldStatus = null;
      delayed.complete(await gateway.lan.hostStatus());
      await tester.pump();
      await waitFor(tester, find.byKey(const Key('remote-screen')));
      expect((await lan.snapshotHostStatus()).active, isTrue);
      final texture = tester.widget<Texture>(find.byType(Texture)).textureId;
      var rendered = 0;
      for (var i = 0; i < 50 && rendered < 5; i++) {
        await tester.pump(const Duration(milliseconds: 100));
        rendered = (await NativeLanGateway.videoChannel.invokeMethod<int>(
          'renderedFrames',
          texture,
        ))!;
      }
      expect(
        rendered,
        greaterThanOrEqualTo(5),
        reason:
            'Native raster callback must render multiple decoded H.264 frames',
      );
      await tester.tap(find.text('Ngắt kết nối'));
      await waitFor(tester, find.text('Kết nối và điều khiển'));
      for (var i = 0; i < 30 && (await lan.snapshotHostStatus()).active; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect((await lan.snapshotHostStatus()).active, isFalse);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
