import 'package:flutter/material.dart';

import 'app.dart';
import 'engine_gateway.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(BeoDeskApp(gateway: NativeEngineGateway()));
}
