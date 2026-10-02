import 'dart:async';

import 'package:beodesk/lan_gateway.dart';
import 'package:beodesk/lan_panel.dart';
import 'package:beodesk/connection_details.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

class FakeLan implements LanGateway {
  @override
  bool get canHost => true;
  List<LocalLanAddress> addresses = const [
    LocalLanAddress(ip: '192.168.1.20', interfaceName: 'eth0'),
  ];
  @override
  Future<List<LocalLanAddress>> localAddresses() async => addresses;
  LanHostState state = const LanHostState(listening: false);
  final responses = <(int, bool)>[];
  int stopped = 0;
  int cancelled = 0;
  final image = Completer<Uint8List>();

  @override
  Future<LanHostState> startHost(String address, String peerFingerprint) async {
    state = LanHostState(listening: true, address: address);
    return state;
  }

  @override
  Future<LanHostState> hostStatus() async => state;
  @override
  Future<void> stopHost() async {
    stopped++;
    state = const LanHostState(listening: false);
  }

  @override
  Future<void> respond(int requestId, bool approved) async {
    responses.add((requestId, approved));
    state = const LanHostState(listening: true, address: '127.0.0.1:4433');
  }

  @override
  Future<Uint8List> fetch(String address, String hostFingerprint) =>
      image.future;
  @override
  Future<void> cancel() async {
    cancelled++;
    if (!image.isCompleted) {
      image.completeError(StateError('Request cancelled'));
    }
  }
}

Future<void> mount(WidgetTester tester, FakeLan gateway) => tester.pumpWidget(
  MaterialApp(
    home: Scaffold(
      body: SingleChildScrollView(
        child: LanPanel(
          gateway: gateway,
          localFingerprint: 'AAAAAAAA AAAAAAAA AAAAAAAA AAAAAAAA AAAAAAAA AAAAAAAA AAAAAAAA AAAAAAAA',
        ),
      ),
    ),
  ),
);

Future<void> startHost(WidgetTester tester) async {
  await tester.ensureVisible(find.text('Chia sẻ màn hình máy này'));
  await tester.tap(find.text('Chia sẻ màn hình máy này'));
  await tester.pumpAndSettle();
  await tester.tap(find.byKey(const Key('detect-host-address')));
  await tester.pumpAndSettle();
  await tester.enterText(
    find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
    'a' * 64,
  );
  await tester.tap(find.text('Bật chia sẻ'));
  await tester.pumpAndSettle();
  await tester.pump(const Duration(milliseconds: 400));
}

const pending = LanHostState(
  listening: true,
  address: '127.0.0.1:4433',
  requestId: 7,
  peerFingerprint:
      '12345678 '
      '12345678 '
      '12345678 '
      '12345678 '
      '12345678 '
      '12345678 '
      '12345678 '
      '12345678',
  deviceName: 'Verified viewer',
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  String? clipboard;
  setUp(() {
    clipboard = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.getData') {
            return clipboard == null ? null : {'text': clipboard};
          }
          if (call.method == 'Clipboard.setData') {
            clipboard = (call.arguments as Map)['text'] as String;
          }
          if (call.method == 'Clipboard.hasStrings') {
            return {'value': clipboard?.isNotEmpty ?? false};
          }
          return null;
        });
  });
  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.platform, null),
  );

  testWidgets('received packet fills the viewer address and full host pin', (
    tester,
  ) async {
    await mount(tester, FakeLan());
    clipboard = LanConnectionDetails(
      address: '172.16.1.126:4433',
      fingerprint: 'b' * 64,
    ).encode();
    await tester.tap(find.byKey(const Key('paste-remote-details')));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<TextField>(
            find.widgetWithText(TextField, 'IP máy chia sẻ và cổng'),
          )
          .controller!
          .text,
      '172.16.1.126:4433',
    );
    expect(
      tester
          .widget<TextField>(
            find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
          )
          .controller!
          .text
          .replaceAll(' ', ''),
      'B' * 64,
    );
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'popup pastes only the viewer pin and copies the local device pin',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await tester.ensureVisible(
        find.byKey(const Key('detect-local-endpoint')),
      );
      await tester.tap(find.byKey(const Key('detect-local-endpoint')));
      await tester.pumpAndSettle();
      clipboard = LanConnectionDetails(
        address: '172.16.1.99:5444',
        fingerprint: 'b' * 64,
      ).encode();
      await tester.ensureVisible(
        find.byKey(const Key('paste-viewer-fingerprint')),
      );
      await tester.tap(find.byKey(const Key('paste-viewer-fingerprint')));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(
              find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
            )
            .controller!
            .text
            .replaceAll(' ', ''),
        'B' * 64,
      );
      expect(
        tester
            .widget<TextField>(
              find.widgetWithText(TextField, 'IP máy này và cổng'),
            )
            .controller!
            .text,
        '192.168.1.20:4433',
      );
      await tester.ensureVisible(find.byKey(const Key('copy-local-details')));
      await tester.tap(find.byKey(const Key('copy-local-details')));
      await tester.pumpAndSettle();
      final copied = LanConnectionDetails.parse(clipboard!);
      expect(copied.address, '192.168.1.20:4433');
      expect(copied.fingerprint.replaceAll(' ', ''), 'A' * 64);
      expect(gateway.state.listening, isFalse);
      await tester.tap(find.text('Hủy'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'malformed pasted details do not partly replace existing fields',
    (tester) async {
      await mount(tester, FakeLan());
      final address = find.widgetWithText(TextField, 'IP máy chia sẻ và cổng');
      final pin = find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ');
      await tester.enterText(address, '192.168.1.10:4433');
      await tester.enterText(pin, 'c' * 64);
      clipboard = 'BeoDesk\nIP: 172.16.1.126:4433\nDấu vân tay: 12345678';
      await tester.tap(find.byKey(const Key('paste-remote-details')));
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(address).controller!.text,
        '192.168.1.10:4433',
      );
      expect(tester.widget<TextField>(pin).controller!.text, 'c' * 64);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('home IP button detects its address without starting sharing', (
    tester,
  ) async {
    final gateway = FakeLan();
    await mount(tester, gateway);
    final button = find.byKey(const Key('detect-local-endpoint'));
    await tester.ensureVisible(button);
    await tester.tap(button);
    await tester.pumpAndSettle();
    final field = find.widgetWithText(TextField, 'IP máy này và cổng');
    expect(
      tester.widget<TextField>(field).controller!.text,
      '192.168.1.20:4433',
    );
    expect(gateway.state.listening, isFalse);
    final remote = find.widgetWithText(TextField, 'IP máy chia sẻ và cổng');
    expect(tester.widget<TextField>(remote).controller!.text, '127.0.0.1:4433');
    await tester.tap(find.text('Hủy'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'local IP button fills the endpoint and preserves a custom port',
    (tester) async {
      await mount(tester, FakeLan());
      await tester.ensureVisible(find.text('Chia sẻ màn hình máy này'));
      await tester.tap(find.text('Chia sẻ màn hình máy này'));
      await tester.pumpAndSettle();
      final field = find.widgetWithText(TextField, 'IP máy này và cổng');
      await tester.tap(find.byKey(const Key('detect-host-address')));
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(field).controller!.text,
        '192.168.1.20:4433',
      );
      await tester.enterText(field, '127.0.0.1:5444');
      await tester.tap(find.byKey(const Key('detect-host-address')));
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextField>(field).controller!.text,
        '192.168.1.20:5444',
      );
      await tester.tap(find.text('Hủy'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('multiple interfaces require choosing the intended LAN address', (
    tester,
  ) async {
    final gateway = FakeLan()
      ..addresses = const [
        LocalLanAddress(ip: '192.168.1.20', interfaceName: 'eth0'),
        LocalLanAddress(ip: '172.16.1.126', interfaceName: 'wlan0'),
      ];
    await mount(tester, gateway);
    await tester.ensureVisible(find.text('Chia sẻ màn hình máy này'));
    await tester.tap(find.text('Chia sẻ màn hình máy này'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('detect-host-address')));
    await tester.pumpAndSettle();
    expect(find.text('Chọn IP máy này'), findsOneWidget);
    await tester.tap(find.text('172.16.1.126'));
    await tester.pumpAndSettle();
    final field = find.widgetWithText(TextField, 'IP máy này và cổng');
    expect(
      tester.widget<TextField>(field).controller!.text,
      '172.16.1.126:4433',
    );
    await tester.tap(find.text('Hủy'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('no LAN address keeps manual entry available', (tester) async {
    await mount(tester, FakeLan()..addresses = []);
    await tester.ensureVisible(find.text('Chia sẻ màn hình máy này'));
    await tester.tap(find.text('Chia sẻ màn hình máy này'));
    await tester.pumpAndSettle();
    final field = find.widgetWithText(TextField, 'IP máy này và cổng');
    await tester.enterText(field, '192.168.1.50:4433');
    await tester.tap(find.byKey(const Key('detect-host-address')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Không tìm thấy IP mạng.'), findsOneWidget);
    expect(
      tester.widget<TextField>(field).controller!.text,
      '192.168.1.50:4433',
    );
    await tester.tap(find.text('Hủy'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('connection loss keeps the full native cause available', (
    tester,
  ) async {
    final gateway = FakeLan();
    await mount(tester, gateway);
    await tester.enterText(
      find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
      'a' * 64,
    );
    await tester.tap(find.text('Nhận ảnh màn hình'));
    await tester.pump();
    const details =
        'Waiting for host approval\n\nCaused by:\n'
        '    connection lost: transport error: peer rejected certificate';
    gateway.image.completeError(AnyhowException(details));
    await tester.pumpAndSettle();
    expect(find.textContaining('Kết nối bị ngắt.'), findsOneWidget);
    expect(find.textContaining('AnyhowException('), findsNothing);
    await tester.tap(find.text('Chi tiết'));
    await tester.pumpAndSettle();
    expect(find.text(details), findsOneWidget);
    expect(find.byType(SelectableText), findsOneWidget);
    await tester.tap(find.text('Đóng'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('host waits for local consent and denial is explicit', (
    tester,
  ) async {
    final gateway = FakeLan();
    await mount(tester, gateway);
    await startHost(tester);
    gateway.state = pending;
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(find.text('Cho phép xem màn hình?'), findsOneWidget);
    expect(find.text(pending.peerFingerprint), findsOneWidget);
    expect(gateway.responses, isEmpty);
    await tester.tap(find.text('Từ chối'));
    await tester.pumpAndSettle();
    expect(gateway.responses, [(7, false)]);
    await tester.pumpWidget(const SizedBox());
    expect(gateway.stopped, 1);
  });

  testWidgets('expired request closes its prompt without approving', (
    tester,
  ) async {
    final gateway = FakeLan();
    await mount(tester, gateway);
    await startHost(tester);
    gateway.state = pending;
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    gateway.state = const LanHostState(listening: true);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(find.text('Cho phép xem màn hình?'), findsNothing);
    expect(gateway.responses, isEmpty);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('pending viewer can cancel and clears its progress state', (
    tester,
  ) async {
    final gateway = FakeLan();
    await mount(tester, gateway);
    await tester.enterText(
      find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
      'a' * 64,
    );
    await tester.tap(find.text('Nhận ảnh màn hình'));
    await tester.pump();
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    await tester.ensureVisible(find.text('Hủy yêu cầu'));
    await tester.tap(find.text('Hủy yêu cầu'));
    await tester.pumpAndSettle();
    expect(gateway.cancelled, 1);
    expect(find.byType(LinearProgressIndicator), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('LAN form fits a narrow viewport', (tester) async {
    tester.view.physicalSize = const Size(400, 850);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await mount(tester, FakeLan());
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });
}
