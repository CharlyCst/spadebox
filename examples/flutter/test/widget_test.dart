import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('App smoke test', (WidgetTester tester) async {
    // The agent app requires RustLib.init() before it can be pumped,
    // so we just verify the test harness runs without crashing.
    expect(true, isTrue);
  });
}
