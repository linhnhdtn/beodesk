import 'dart:async';

import 'package:beodesk/lan_gateway.dart';
import 'package:beodesk/live_session.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'lan_panel_test.dart' show FakeLan, mount, startHost;

Future<void> paintFrame(
  WidgetTester tester,
  FakeLan gateway,
  Color color,
) async {
  gateway.frames.add(const LanFrame(frameId: 1, width: 400, height: 200));
  await tester.pump(const Duration(milliseconds: 110));
}

Future<void> viewer(WidgetTester tester, FakeLan gateway) async {
  await tester.pumpWidget(
    MaterialApp(home: LiveSession(gateway: gateway, sessionId: 9)),
  );
  await paintFrame(tester, gateway, Colors.red);
  expect(find.byKey(const Key('remote-screen')), findsOneWidget);
}

class DelayedTextureLan extends FakeLan {
  final texture = Completer<int>();
  @override
  Future<int> createTexture(int sessionId) => texture.future;
}

void main() {
  testWidgets('closing before native texture creation completes releases it', (
    tester,
  ) async {
    final gateway = DelayedTextureLan();
    await tester.pumpWidget(
      MaterialApp(home: LiveSession(gateway: gateway, sessionId: 9)),
    );
    await tester.pumpWidget(const SizedBox());
    gateway.texture.complete(42);
    await tester.pump();
    expect(gateway.stoppedSessions, contains(9));
    expect(gateway.disposedTextures, [42]);
    expect(tester.takeException(), isNull);
  });

  testWidgets('native texture failure closes the approved session', (
    tester,
  ) async {
    final gateway = DelayedTextureLan();
    await tester.pumpWidget(
      MaterialApp(home: LiveSession(gateway: gateway, sessionId: 9)),
    );
    gateway.texture.completeError(StateError('Texture unavailable'));
    await tester.pump();
    expect(find.text('Phiên đã kết thúc'), findsOneWidget);
    expect(gateway.stoppedSessions, contains(9));
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('live frames reuse one native texture and dispose it on exit', (
    tester,
  ) async {
    final gateway = FakeLan();
    await viewer(tester, gateway);
    final first = tester.widget<Texture>(find.byType(Texture)).textureId;
    await paintFrame(tester, gateway, Colors.blue);
    expect(tester.widget<Texture>(find.byType(Texture)).textureId, first);
    expect(first, 42);
    await tester.pumpWidget(const SizedBox());
    expect(gateway.stoppedSessions, contains(9));
    expect(gateway.disposedTextures, contains(42));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'letterboxing maps clicks and drags to normalized host coordinates',
    (tester) async {
      final gateway = FakeLan();
      await viewer(tester, gateway);
      final rect = tester.getRect(find.byKey(const Key('remote-screen')));
      expect(rect.width / rect.height, closeTo(2, 0.001));
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.down(
        rect.topLeft + Offset(rect.width * .25, rect.height * .75),
      );
      await tester.pump();
      await gesture.moveTo(rect.center);
      await tester.pump(const Duration(milliseconds: 20));
      await gesture.up();
      await tester.pump();
      final buttons = gateway.inputs.where((e) => e.kind == 1).toList();
      expect(buttons.length, 2);
      expect(buttons[0].pressed, isTrue);
      expect(buttons[0].code, 1);
      expect(buttons[0].x, closeTo(.25, .001));
      expect(buttons[0].y, closeTo(.75, .001));
      expect(buttons[1].pressed, isFalse);
      expect(buttons[1].x, closeTo(.5, .001));
      expect(buttons[1].y, closeTo(.5, .001));
      expect(gateway.inputs.where((e) => e.kind == 0), isNotEmpty);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'physical keys preserve modifiers and release when control is disabled',
    (tester) async {
      final gateway = FakeLan();
      await viewer(tester, gateway);
      await tester.tap(find.byKey(const Key('remote-screen')));
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyA);
      await tester.sendKeyRepeatEvent(LogicalKeyboardKey.keyA);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyA);
      await tester.pump();
      final keys = gateway.inputs.where((e) => e.kind == 2).toList();
      expect(keys.map((e) => (e.code, e.pressed)), [
        (225, true),
        (4, true),
        (4, false),
      ]);
      await tester.tap(find.text('Điều khiển'));
      await tester.pump();
      expect(gateway.inputs.any((e) => e.kind == 4), isTrue);
      final count = gateway.inputs.length;
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyB);
      await tester.pump();
      expect(gateway.inputs.length, count);
      expect(find.text('Chỉ xem'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'application focus loss releases input and remote close ends the viewer',
    (tester) async {
      final gateway = FakeLan();
      await viewer(tester, gateway);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      expect(gateway.inputs.last.kind, 4);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      gateway.frames.add(const LanFrame(closed: true));
      await tester.pump(const Duration(milliseconds: 110));
      expect(find.text('Phiên đã kết thúc'), findsOneWidget);
      expect(find.byKey(const Key('remote-screen')), findsNothing);
      expect(gateway.stoppedSessions, contains(9));
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'host consent explicitly requests continuous viewing and control',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await startHost(tester);
      gateway.state = const LanHostState(
        listening: true,
        requestId: 8,
        live: true,
        control: true,
        deviceName: 'Viewer',
      );
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pumpAndSettle();
      expect(find.text('Cho phép xem và điều khiển?'), findsOneWidget);
      expect(find.textContaining('điều khiển chuột, bàn phím'), findsOneWidget);
      expect(gateway.responses, isEmpty);
      await tester.tap(find.text('Cho phép điều khiển'));
      await tester.pumpAndSettle();
      expect(gateway.responses, [(8, true)]);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('long connection errors remain within the viewer window', (
    tester,
  ) async {
    final gateway = FakeLan();
    await viewer(tester, gateway);
    gateway.frames.add(
      LanFrame(
        closed: true,
        error: List.filled(
          100,
          'Connection closed with diagnostic details',
        ).join('\n'),
      ),
    );
    await tester.pump(const Duration(milliseconds: 110));
    expect(find.text('Phiên đã kết thúc'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });
}
