import 'dart:async';

import 'package:beodesk/app.dart';
import 'package:beodesk/engine_gateway.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const device = DeviceSnapshot(
  version: '0.1.0',
  platform: 'linux',
  architecture: 'x86_64',
  fingerprint: 'ABCD1234 EFGH5678',
  storage: 'test store',
);

class FakeGateway extends EngineGateway {
  FakeGateway(this.load);
  final Future<DeviceSnapshot> Function() load;
  @override
  Future<DeviceSnapshot> initialize() => load();
}

void main() {
  testWidgets('does not offer a connection before initialization completes', (
    tester,
  ) async {
    final pending = Completer<DeviceSnapshot>();
    await tester.pumpWidget(
      BeoDeskApp(gateway: FakeGateway(() => pending.future)),
    );
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('Sao chép'), findsNothing);
    pending.complete(device);
    await tester.pumpAndSettle();
    expect(find.text(device.fingerprint!), findsOneWidget);
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Kết nối'),
    );
    expect(button.onPressed, isNull);
  });

  testWidgets('engine startup can recover after an error', (tester) async {
    var attempts = 0;
    await tester.pumpWidget(
      BeoDeskApp(
        gateway: FakeGateway(() async {
          if (attempts++ == 0) throw StateError('native load failed');
          return device;
        }),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Không khởi động được BeoDesk.'), findsOneWidget);
    await tester.tap(find.text('Thử lại'));
    await tester.pumpAndSettle();
    expect(find.text(device.fingerprint!), findsOneWidget);
  });

  testWidgets('unavailable secure storage never displays a pretend identity', (
    tester,
  ) async {
    await tester.pumpWidget(
      BeoDeskApp(
        gateway: FakeGateway(
          () async => const DeviceSnapshot(
            version: '0.1.0',
            platform: 'linux',
            architecture: 'x86_64',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Chưa mở được kho khóa thiết bị'), findsOneWidget);
    expect(find.text('Sao chép'), findsNothing);
  });

  testWidgets('narrow layout remains scrollable without overflow', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(400, 850);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(
      BeoDeskApp(gateway: FakeGateway(() async => device)),
    );
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(find.byType(SingleChildScrollView), findsWidgets);
  });
}
