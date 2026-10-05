import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'engine_gateway.dart';
import 'lan_panel.dart';

class BeoDeskApp extends StatelessWidget {
  const BeoDeskApp({super.key, required this.gateway});
  final EngineGateway gateway;

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'BeoDesk',
    debugShowCheckedModeBanner: false,
    theme: ThemeData(
      useMaterial3: true,
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff087f8c)),
      scaffoldBackgroundColor: const Color(0xfff4f7fa),
      inputDecorationTheme: const InputDecorationTheme(
        border: OutlineInputBorder(),
      ),
    ),
    home: DeviceHome(gateway: gateway),
  );
}

class DeviceHome extends StatefulWidget {
  const DeviceHome({super.key, required this.gateway});
  final EngineGateway gateway;
  @override
  State<DeviceHome> createState() => _DeviceHomeState();
}

class _DeviceHomeState extends State<DeviceHome> {
  late Future<DeviceSnapshot> _initialization;

  @override
  void initState() {
    super.initState();
    _initialization = widget.gateway.initialize();
  }

  void _retry() {
    final initialization = widget.gateway.initialize();
    setState(() {
      _initialization = initialization;
    });
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Row(
        children: [
          Icon(Icons.desktop_windows_rounded, color: Color(0xff087f8c)),
          SizedBox(width: 12),
          Expanded(
            child: Text(
              'BeoDesk',
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontWeight: FontWeight.w700),
            ),
          ),
        ],
      ),
      actions: const [
        Padding(
          padding: EdgeInsets.only(right: 20),
          child: Chip(
            avatar: Icon(Icons.science_outlined, size: 16),
            label: Text('Bản phát triển'),
          ),
        ),
      ],
    ),
    body: FutureBuilder<DeviceSnapshot>(
      future: _initialization,
      builder: (context, snapshot) {
        if (snapshot.connectionState != ConnectionState.done) {
          return const Center(
            child: Padding(
              padding: EdgeInsets.all(24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 20),
                  Text('Đang chuẩn bị thiết bị…'),
                  SizedBox(height: 8),
                  Text(
                    'Mở khóa kho mật khẩu nếu hệ điều hành yêu cầu.',
                    textAlign: TextAlign.center,
                  ),
                ],
              ),
            ),
          );
        }
        if (snapshot.hasError || !snapshot.hasData) {
          return Center(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.error_outline, size: 40),
                  const SizedBox(height: 16),
                  const Text('Không khởi động được BeoDesk.'),
                  const SizedBox(height: 8),
                  const Text('Thử lại hoặc kiểm tra bản cài đặt ứng dụng.'),
                  const SizedBox(height: 20),
                  FilledButton(onPressed: _retry, child: const Text('Thử lại')),
                ],
              ),
            ),
          );
        }
        return _content(snapshot.requireData);
      },
    ),
  );

  Widget _content(DeviceSnapshot device) => SingleChildScrollView(
    padding: const EdgeInsets.all(24),
    child: Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1000),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 20),
            Text(
              'Không gian làm việc của bạn',
              style: Theme.of(context).textTheme.headlineMedium
                  ?.copyWith(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 10),
            const Text('Quản lý thiết bị và kết nối đến máy tính của bạn.'),
            const SizedBox(height: 28),
            LayoutBuilder(
              builder: (context, constraints) {
                final local = _deviceCard(device);
                final lan = widget.gateway.lan;
                final remote = device.fingerprint != null && lan != null
                    ? LanPanel(
                        gateway: lan,
                        localFingerprint: device.fingerprint!,
                      )
                    : _connectCard();
                final narrow = constraints.maxWidth < 720;
                final cardWidth = narrow
                    ? constraints.maxWidth
                    : (constraints.maxWidth - 20) / 2;
                // Keep the same element hierarchy across the breakpoint:
                // disposing LanPanel would stop its active host/viewer session.
                return Flex(
                  direction: narrow ? Axis.vertical : Axis.horizontal,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(width: cardWidth, child: local),
                    SizedBox(width: narrow ? 0 : 20, height: narrow ? 20 : 0),
                    SizedBox(width: cardWidth, child: remote),
                  ],
                );
              },
            ),
            const SizedBox(height: 24),
            _card(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Icon(Icons.shield_outlined, color: Color(0xff087f8c)),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text(
                          'Bạn luôn có quyền kiểm soát',
                          style: TextStyle(fontWeight: FontWeight.w600),
                        ),
                        const SizedBox(height: 6),
                        const Text(
                          'Mỗi yêu cầu xem màn hình cần được xác nhận tại máy chia sẻ. Khóa riêng của thiết bị không được gửi ra ngoài.',
                        ),
                        const SizedBox(height: 12),
                        Text(
                          'BeoDesk ${device.version} · ${device.platform} · ${device.architecture}',
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    ),
  );

  Widget _deviceCard(DeviceSnapshot device) {
    final ready = device.fingerprint != null;
    return _card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(
            Icons.computer_rounded,
            size: 32,
            color: Color(0xff087f8c),
          ),
          const SizedBox(height: 16),
          Text('Máy của bạn', style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: 12),
          Text(
            ready
                ? 'Đã tạo định danh an toàn'
                : 'Chưa mở được kho khóa thiết bị',
            style: TextStyle(
              color: ready
                  ? const Color(0xff087f8c)
                  : Colors.deepOrange.shade800,
            ),
          ),
          const SizedBox(height: 20),
          if (ready) ...[
            const Text(
              'Dấu vân tay thiết bị',
              style: TextStyle(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            SelectableText(
              device.fingerprint!,
              style: const TextStyle(
                fontFamily: 'monospace',
                fontSize: 13,
                height: 1.7,
              ),
            ),
            const SizedBox(height: 12),
            OutlinedButton.icon(
              onPressed: () async {
                await Clipboard.setData(
                  ClipboardData(text: device.fingerprint!),
                );
                if (!mounted) return;
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Đã sao chép dấu vân tay.')),
                );
              },
              icon: const Icon(Icons.copy_rounded, size: 18),
              label: const Text('Sao chép'),
            ),
            const SizedBox(height: 8),
            Text(
              'Khóa riêng được giữ trong ${device.storage}.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ] else ...[
            const Text(
              'Mở khóa kho mật khẩu của hệ điều hành rồi thử lại. Ứng dụng sẽ không tạo khóa tạm thay cho định danh đã lưu. Trên nền tảng chưa hỗ trợ lưu khóa, tính năng này sẽ được bổ sung sau.',
            ),
            const SizedBox(height: 16),
            OutlinedButton(onPressed: _retry, child: const Text('Thử lại')),
          ],
        ],
      ),
    );
  }

  Widget _connectCard() => _card(
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Icon(Icons.devices_rounded, size: 32, color: Color(0xff087f8c)),
        const SizedBox(height: 16),
        Text(
          'Kết nối đến thiết bị',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: 12),
        const Text(
          'Cần mở được kho khóa thiết bị để kết nối. Chức năng này hiện chưa khả dụng trên thiết bị này.',
        ),
        const SizedBox(height: 24),
        const TextField(
          enabled: false,
          decoration: InputDecoration(
            labelText: 'Địa chỉ IP của máy đích',
            hintText: '192.168.1.20',
            prefixIcon: Icon(Icons.lan_outlined),
          ),
        ),
        const SizedBox(height: 16),
        SizedBox(
          width: double.infinity,
          child: FilledButton.icon(
            onPressed: null,
            icon: const Icon(Icons.arrow_forward_rounded),
            label: const Text('Kết nối'),
          ),
        ),
      ],
    ),
  );

  Widget _card({required Widget child}) => Container(
    width: double.infinity,
    padding: const EdgeInsets.all(24),
    decoration: BoxDecoration(
      color: Colors.white,
      borderRadius: BorderRadius.circular(18),
      border: Border.all(color: const Color(0xffdfe6ec)),
    ),
    child: child,
  );
}
