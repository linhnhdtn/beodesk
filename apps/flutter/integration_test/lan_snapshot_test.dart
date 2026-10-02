// Requires a real, unlocked GNOME Secret Service session. Run only under Xvfb.
import 'dart:io';

import 'package:beodesk/app.dart';
import 'package:beodesk/engine_gateway.dart';
import 'package:beodesk/src/rust/api/app.dart' as native;
import 'package:beodesk/src/rust/api/lan.dart' as lan;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

Future<void> waitFor(WidgetTester tester, Finder finder) async {
  for (var attempt = 0; attempt < 150; attempt++) {
    await tester.pump(const Duration(milliseconds: 100));
    if (finder.evaluate().isNotEmpty) return;
  }
  fail('Timed out waiting for $finder');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'native LAN request requires UI consent and displays Xvfb capture',
    (tester) async {
      final gateway = NativeEngineGateway();
      final device = await gateway.initialize();
      expect(device.fingerprint, isNotNull);
      final addresses = await gateway.lan!.localAddresses();
      expect(addresses, isNotEmpty);
      for (final address in addresses) {
        expect(InternetAddress(address.ip).isLoopback, isFalse);
      }
      final localIp = addresses.first.ip;
      final socket = await RawDatagramSocket.bind(InternetAddress(localIp), 0);
      final port = socket.port;
      final address = '$localIp:$port';
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
      await tester.enterText(
        find.widgetWithText(TextField, 'IP máy này và cổng'),
        '127.0.0.1:$port',
      );
      await tester.tap(find.byKey(const Key('detect-host-address')));
      if (addresses.length > 1) {
        await waitFor(tester, find.text('Chọn IP máy này'));
        await tester.tap(find.text(localIp));
      }
      await waitFor(tester, find.text(address));
      await tester.enterText(
        find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
        device.fingerprint!,
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
      await tester.ensureVisible(find.text('Nhận ảnh màn hình'));
      await tester.tap(find.text('Nhận ảnh màn hình'));
      await waitFor(tester, find.text('Cho phép xem màn hình?'));
      expect(find.text('Ảnh màn hình đã nhận'), findsNothing);
      expect((await lan.snapshotHostStatus()).requestId, greaterThan(0));
      await tester.tap(find.text('Cho phép một ảnh'));
      await waitFor(tester, find.text('Ảnh màn hình đã nhận'));
      expect(
        find.byType(LinearProgressIndicator, skipOffstage: false),
        findsNothing,
      );
      await tester.pumpAndSettle();
      expect(find.byType(Image), findsOneWidget);
      expect(tester.takeException(), isNull);
      expect(native.engineInfo().canHost, isTrue);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
