import 'package:beodesk/app.dart';
import 'package:beodesk/engine_gateway.dart';
import 'package:beodesk/lan_gateway.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'lan_panel_test.dart' show FakeLan, startHost;

class _Gateway extends EngineGateway {
  _Gateway(this.lan);
  @override
  final FakeLan lan;
  @override
  Future<DeviceSnapshot> initialize() async => DeviceSnapshot(
    version: 'test',
    platform: 'linux',
    architecture: 'x86_64',
    fingerprint: 'a' * 64,
    storage: 'test',
  );
}

class _Lan extends FakeLan {
  @override
  Future<void> cancel() async {
    cancelled++;
    if (!live.isCompleted) live.completeError(StateError('Request cancelled'));
  }
}

Future<void> resize(WidgetTester tester, double width) async {
  tester.view.physicalSize = Size(width, 900);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

Future<void> mount(WidgetTester tester, FakeLan lan) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1280, 900);
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(BeoDeskApp(gateway: _Gateway(lan)));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('resizing the host preserves sharing, consent and form fields', (
    tester,
  ) async {
    final lan = _Lan();
    await mount(tester, lan);
    await startHost(tester);
    await tester.enterText(
      find.widgetWithText(TextField, 'IP máy chia sẻ và cổng'),
      '192.168.1.25:4433',
    );
    for (final width in [400.0, 1280.0, 600.0, 1000.0]) {
      await resize(tester, width);
      expect(lan.stopped, 0, reason: 'A layout change must not stop the host');
      expect(find.text('Dừng chia sẻ'), findsOneWidget);
      expect(find.text('192.168.1.25:4433'), findsOneWidget);
    }
    lan.state = const LanHostState(
      listening: true,
      requestId: 8,
      live: true,
      control: true,
    );
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    await resize(tester, 400);
    expect(find.text('Cho phép xem và điều khiển?'), findsOneWidget);
    expect(lan.stopped, 0);
    await tester.tap(find.text('Cho phép điều khiển'));
    await tester.pumpAndSettle();
    expect(lan.responses, [(8, true)]);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    expect(lan.stopped, 1, reason: 'Removing the app still stops sharing');
  });

  testWidgets('resizing while connecting does not cancel the viewer', (
    tester,
  ) async {
    final lan = _Lan();
    await mount(tester, lan);
    await tester.enterText(
      find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
      'b' * 64,
    );
    await tester.tap(find.text('Kết nối và điều khiển'));
    await tester.pump();
    await resize(tester, 400);
    expect(lan.cancelled, 0);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    await resize(tester, 1280);
    expect(lan.cancelled, 0);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    expect(lan.cancelled, 1);
    expect(tester.takeException(), isNull);
  });

  testWidgets('resizing during a live session keeps both peers running', (
    tester,
  ) async {
    final lan = _Lan();
    await mount(tester, lan);
    await startHost(tester);
    await tester.enterText(
      find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
      'b' * 64,
    );
    await tester.ensureVisible(find.text('Kết nối và điều khiển'));
    await tester.tap(find.text('Kết nối và điều khiển'));
    await tester.pump();
    lan.live.complete(9);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    for (final width in [400.0, 1280.0]) {
      await resize(tester, width);
      expect(lan.cancelled, 0);
      expect(lan.stopped, 0);
      expect(lan.stoppedSessions, isEmpty);
      expect(find.text('Màn hình trực tiếp'), findsOneWidget);
    }
    await tester.tap(find.text('Ngắt kết nối'));
    await tester.pumpAndSettle();
    expect(lan.stoppedSessions, contains(9));
    expect(
      lan.stopped,
      0,
      reason: 'Viewer disconnect does not stop the host listener',
    );
    await tester.pumpWidget(const SizedBox());
    expect(tester.takeException(), isNull);
  });
}
