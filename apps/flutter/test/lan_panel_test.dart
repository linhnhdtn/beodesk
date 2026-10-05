import 'dart:async';

import 'package:beodesk/lan_gateway.dart';
import 'package:beodesk/lan_panel.dart';
import 'package:beodesk/connection_details.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

class FakeLan implements LanGateway {
  final disposedTextures = <int>[];
  @override
  Future<int> createTexture(int sessionId) async => 42;
  @override
  Future<void> disposeTexture(int textureId) async {
    disposedTextures.add(textureId);
  }

  @override
  bool get canHost => true;
  List<LocalLanAddress> addresses = const [
    LocalLanAddress(ip: '192.168.1.20', interfaceName: 'eth0'),
  ];
  @override
  Future<List<LocalLanAddress>> localAddresses() async => addresses;
  LanHostState state = const LanHostState(listening: false);
  final responses = <(int, bool)>[];
  final fetches = <(String, String)>[];
  int stopped = 0;
  int cancelled = 0;
  var image = Completer<Uint8List>();
  var live = Completer<int>();
  final inputs = <LanInput>[];
  final stoppedSessions = <int>[];
  final frames = <LanFrame>[];

  @override
  Future<int> startLive(String address, String hostFingerprint) {
    fetches.add((address, hostFingerprint));
    return live.future;
  }

  @override
  Future<LanFrame> pollLive(int sessionId) async =>
      frames.isEmpty ? const LanFrame() : frames.removeAt(0);
  @override
  Future<void> sendInput(int sessionId, LanInput input) async {
    inputs.add(input);
  }

  @override
  Future<void> stopLive(int sessionId) async {
    stoppedSessions.add(sessionId);
  }

  @override
  Future<LanHostState> startHost(String address) async {
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
  Future<Uint8List> fetch(String address, String hostFingerprint) {
    fetches.add((address, hostFingerprint));
    return image.future;
  }

  @override
  Future<void> cancel() async {
    cancelled++;
    if (!image.isCompleted) {
      image.completeError(StateError('Request cancelled'));
    }
  }
}

class DelayedStatusLan extends FakeLan {
  Completer<LanHostState>? delayedStatus;
  @override
  Future<LanHostState> hostStatus() =>
      delayedStatus?.future ?? super.hostStatus();
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
  expect(
    find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
    findsNothing,
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
  testWidgets('late consent status cannot pop the host page after approval', (
    tester,
  ) async {
    final gateway = DelayedStatusLan();
    final navigator = GlobalKey<NavigatorState>();
    await tester.pumpWidget(
      MaterialApp(
        navigatorKey: navigator,
        home: const Scaffold(body: Text('Landing page')),
      ),
    );
    unawaited(
      navigator.currentState!.push(
        MaterialPageRoute<void>(
          builder: (_) => Scaffold(
            body: SingleChildScrollView(
              child: LanPanel(gateway: gateway, localFingerprint: 'a' * 64),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await startHost(tester);
    gateway.state = const LanHostState(
      listening: true,
      requestId: 7,
      live: true,
      control: true,
    );
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpAndSettle();
    expect(find.text('Cho phép xem và điều khiển?'), findsOneWidget);

    // A native status call starts while consent is open, then completes after
    // approval while the dialog is still mounted for its reverse animation.
    gateway.delayedStatus = Completer<LanHostState>();
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.text('Cho phép điều khiển'));
    await tester.pump();
    expect(gateway.responses, [(7, true)]);
    gateway.delayedStatus!.complete(const LanHostState(listening: true));
    await tester.pumpAndSettle();
    expect(
      gateway.stopped,
      0,
      reason: 'Late status must not close the host page',
    );
    expect(find.byType(LanPanel), findsOneWidget);
    expect(find.text('Landing page'), findsNothing);
    await tester.pumpWidget(const SizedBox());
    expect(gateway.stopped, 1);
  });
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
    'sharing popup copies its own endpoint without requesting a viewer pin',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await tester.ensureVisible(
        find.byKey(const Key('detect-local-endpoint')),
      );
      await tester.tap(find.byKey(const Key('detect-local-endpoint')));
      await tester.pumpAndSettle();
      expect(
        find.widgetWithText(TextField, 'Dấu vân tay của máy xem'),
        findsNothing,
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

  testWidgets('legacy host rejection explains updating the sharing app', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(400, 850);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final gateway = FakeLan();
    await mount(tester, gateway);
    await tester.enterText(
      find.widgetWithText(TextField, 'IP máy chia sẻ và cổng'),
      '172.16.1.126:4433',
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
      'b' * 64,
    );
    await tester.ensureVisible(find.text('Nhận ảnh màn hình'));
    await tester.tap(find.text('Nhận ảnh màn hình'));
    await tester.pump();
    const details =
        'Waiting for host approval; verify the host trusts this viewer\'s full fingerprint\n\n'
        'Caused by:\n'
        '    0: connection lost\n'
        '    1: aborted by peer: the cryptographic handshake failed: error 40: '
        'unexpected error: Peer device fingerprint does not match the verified pin';
    gateway.image.completeError(AnyhowException(details));
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Máy chia sẻ đang dùng cách kết nối cũ.'),
      findsOneWidget,
    );
    expect(find.byKey(const Key('copy-viewer-fingerprint')), findsNothing);
    expect(find.byType(LinearProgressIndicator), findsNothing);
    await tester.tap(find.text('Chi tiết'));
    await tester.pumpAndSettle();
    expect(find.text(details), findsOneWidget);
    await tester.tap(find.text('Đóng'));
    await tester.pumpAndSettle();
    expect(gateway.fetches, [('172.16.1.126:4433', 'b' * 64)]);
    expect(gateway.responses, isEmpty);
    expect(tester.takeException(), isNull);

    // Retrying keeps the host pin and clears the previous failure guidance.
    gateway.image = Completer<Uint8List>();
    await tester.ensureVisible(find.text('Nhận ảnh màn hình'));
    await tester.tap(find.text('Nhận ảnh màn hình'));
    await tester.pump();
    expect(find.byKey(const Key('copy-viewer-fingerprint')), findsNothing);
    expect(gateway.fetches.last, ('172.16.1.126:4433', 'b' * 64));
    expect(gateway.fetches, hasLength(2));
    await tester.ensureVisible(find.text('Hủy yêu cầu'));
    await tester.tap(find.text('Hủy yêu cầu'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'local host pin mismatch directs the viewer to the host details',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await tester.enterText(
        find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
        'b' * 64,
      );
      await tester.tap(find.text('Nhận ảnh màn hình'));
      await tester.pump();
      gateway.image.completeError(
        AnyhowException(
          'Authenticating LAN host at 172.16.1.126:4433\n\nCaused by:\n'
          '    the cryptographic handshake failed: error 40: '
          'unexpected error: Peer device fingerprint does not match the verified pin',
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Dấu vân tay máy chia sẻ không khớp.'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Sao chép lại IP + vân tay từ máy chia sẻ'),
        findsOneWidget,
      );
      expect(find.byKey(const Key('copy-viewer-fingerprint')), findsNothing);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'sharing starts without a viewer pin and copies the active host details',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await startHost(tester);
      expect(gateway.state.listening, isTrue);
      expect(find.byKey(const Key('trusted-viewer-fingerprint')), findsNothing);
      final copy = find.byKey(const Key('copy-host-details'));
      await tester.ensureVisible(copy);
      await tester.tap(copy);
      await tester.pumpAndSettle();
      final details = LanConnectionDetails.parse(clipboard!);
      expect(details.address, '192.168.1.20:4433');
      expect(details.fingerprint.replaceAll(' ', ''), 'A' * 64);
      ScaffoldMessenger.of(tester.element(copy)).removeCurrentSnackBar();
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Dừng chia sẻ'));
      await tester.tap(find.text('Dừng chia sẻ'));
      await tester.pumpAndSettle();
      expect(copy, findsNothing);
      expect(gateway.stopped, 1);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'viewer only enters the host IP and fingerprint for live control',
    (tester) async {
      final gateway = FakeLan();
      await mount(tester, gateway);
      await tester.enterText(
        find.widgetWithText(TextField, 'IP máy chia sẻ và cổng'),
        '192.168.1.20',
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'Dấu vân tay của máy chia sẻ'),
        'b' * 64,
      );
      await tester.tap(find.text('Kết nối và điều khiển'));
      await tester.pump();
      expect(gateway.fetches, [('192.168.1.20:4433', 'b' * 64)]);
      gateway.live.complete(9);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('Màn hình trực tiếp'), findsOneWidget);
      expect(gateway.responses, isEmpty);
      await tester.tap(find.text('Ngắt kết nối'));
      await tester.pumpAndSettle();
      expect(gateway.stoppedSessions, contains(9));
      await tester.pumpWidget(const SizedBox());
    },
  );

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
