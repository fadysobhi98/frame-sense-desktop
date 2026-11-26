import 'package:material_ui/material_ui.dart';

void main() {
  runApp(const FrameSenseApp());
}

class FrameSenseApp extends StatefulWidget {
  const FrameSenseApp({super.key});

  @override
  State<FrameSenseApp> createState() => _FrameSenseAppState();
}

class _FrameSenseAppState extends State<FrameSenseApp> {
  bool useObsidian = false;

  ThemeData get appTheme {
    final seed = useObsidian ? const Color(0xFFC47A50) : const Color(0xFFC7A875);
    final base = ThemeData(
      useMaterial3: true,
      brightness: Brightness.dark,
      colorScheme: ColorScheme.fromSeed(
        seedColor: seed,
        brightness: Brightness.dark,
        surface: useObsidian ? const Color(0xFF191B1F) : const Color(0xFF202630),
      ),
      scaffoldBackgroundColor: useObsidian ? const Color(0xFF111316) : const Color(0xFF171B22),
    );
    return base.copyWith(
      appBarTheme: AppBarTheme(
        backgroundColor: base.scaffoldBackgroundColor,
        foregroundColor: const Color(0xFFECE8E0),
        elevation: 0,
      ),
      cardTheme: CardThemeData(
        color: useObsidian ? const Color(0xFF1B1E22) : const Color(0xFF202630),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Frame Sense',
      theme: appTheme,
      home: FrameSenseHome(
        useObsidian: useObsidian,
        onThemeChanged: (value) => setState(() => useObsidian = value),
      ),
    );
  }
}

class FrameSenseHome extends StatefulWidget {
  const FrameSenseHome({
    super.key,
    required this.useObsidian,
    required this.onThemeChanged,
  });

  final bool useObsidian;
  final ValueChanged<bool> onThemeChanged;

  @override
  State<FrameSenseHome> createState() => _FrameSenseHomeState();
}

class _FrameSenseHomeState extends State<FrameSenseHome> {
  int selectedIndex = 0;

  static const destinations = [
    _Destination('Home', Icons.home_outlined),
    _Destination('Projects', Icons.folder_outlined),
    _Destination('Gallery', Icons.photo_library_outlined),
    _Destination('Edit', Icons.tune),
    _Destination('Devices', Icons.devices_outlined),
  ];

  @override
  Widget build(BuildContext context) {
    final current = destinations[selectedIndex];
    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.camera_enhance_outlined, size: 22),
            SizedBox(width: 10),
            Text('FRAME SENSE', style: TextStyle(fontWeight: FontWeight.w800, letterSpacing: 1.2)),
          ],
        ),
        actions: [
          PopupMenuButton<bool>(
            tooltip: 'Choose theme',
            initialValue: widget.useObsidian,
            onSelected: widget.onThemeChanged,
            itemBuilder: (context) => const [
              PopupMenuItem(value: false, child: Text('Graphite & Champagne')),
              PopupMenuItem(value: true, child: Text('Obsidian & Copper')),
            ],
            icon: const Icon(Icons.palette_outlined),
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: SafeArea(
        child: selectedIndex == 0 ? _home(context) : _planned(context, current),
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: selectedIndex,
        onDestinationSelected: (value) => setState(() => selectedIndex = value),
        destinations: destinations
            .map((item) => NavigationDestination(icon: Icon(item.icon), label: item.label))
            .toList(),
      ),
    );
  }

  Widget _home(BuildContext context) {
    final color = Theme.of(context).colorScheme.primary;
    return ListView(
      padding: const EdgeInsets.all(18),
      children: [
        Text('LOCAL-FIRST PHOTOGRAPHY WORKFLOW',
            style: Theme.of(context).textTheme.labelSmall?.copyWith(color: color, letterSpacing: 1.4)),
        const SizedBox(height: 12),
        Text('Your projects.\\nYour original files. Protected.',
            style: Theme.of(context).textTheme.headlineMedium?.copyWith(fontWeight: FontWeight.w800)),
        const SizedBox(height: 10),
        Text(
          'A mobile foundation for Frame Sense. RAW editing, local AI and secure desktop sync will be added in later verified stages.',
          style: Theme.of(context).textTheme.bodyMedium?.copyWith(color: Colors.white70, height: 1.5),
        ),
        const SizedBox(height: 22),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [Icon(Icons.shield_outlined, color: color), const SizedBox(width: 8), const Text('Core workflow rules')]),
                const SizedBox(height: 14),
                const _Rule('Original RAW remains unchanged'),
                const _Rule('JPEG 8-bit final output'),
                const _Rule('One active order at a time'),
                const _Rule('Camera 1 and Camera 2 stay separate'),
                const _Rule('No mandatory cloud AI'),
              ],
            ),
          ),
        ),
        const SizedBox(height: 14),
        GridView.count(
          crossAxisCount: 2,
          shrinkWrap: true,
          physics: const NeverScrollableScrollPhysics(),
          mainAxisSpacing: 10,
          crossAxisSpacing: 10,
          childAspectRatio: 1.45,
          children: const [
            _FeatureCard(icon: Icons.folder_open, title: 'Projects', subtitle: 'Local project management'),
            _FeatureCard(icon: Icons.photo_library_outlined, title: 'Gallery', subtitle: 'Browse project images'),
            _FeatureCard(icon: Icons.tune, title: 'Edit', subtitle: 'Independent mobile editing'),
            _FeatureCard(icon: Icons.devices, title: 'Devices', subtitle: 'Secure pairing planned'),
          ],
        ),
        const SizedBox(height: 14),
        Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            border: Border.all(color: Theme.of(context).colorScheme.outlineVariant),
            borderRadius: BorderRadius.circular(12),
          ),
          child: const Row(
            children: [
              Icon(Icons.info_outline),
              SizedBox(width: 10),
              Expanded(child: Text('Foundation preview only. No RAW or AI processing is connected yet.')),
            ],
          ),
        ),
      ],
    );
  }

  Widget _planned(BuildContext context, _Destination destination) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(28),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(destination.icon, size: 44, color: Theme.of(context).colorScheme.primary),
            const SizedBox(height: 16),
            Text(destination.label, style: Theme.of(context).textTheme.headlineSmall),
            const SizedBox(height: 10),
            const Text(
              'This module is part of the approved Frame Sense scope and is planned for a later implementation stage.',
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 12),
            const Chip(label: Text('Planned – not implemented')),
          ],
        ),
      ),
    );
  }
}

class _Destination {
  const _Destination(this.label, this.icon);
  final String label;
  final IconData icon;
}

class _Rule extends StatelessWidget {
  const _Rule(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 5),
        child: Row(
          children: [
            Icon(Icons.check_circle_outline, size: 17, color: Theme.of(context).colorScheme.primary),
            const SizedBox(width: 9),
            Expanded(child: Text(text)),
          ],
        ),
      );
}

class _FeatureCard extends StatelessWidget {
  const _FeatureCard({required this.icon, required this.title, required this.subtitle});
  final IconData icon;
  final String title;
  final String subtitle;

  @override
  Widget build(BuildContext context) => Card(
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, color: Theme.of(context).colorScheme.primary),
              const SizedBox(height: 9),
              Text(title, style: const TextStyle(fontWeight: FontWeight.w700)),
              const SizedBox(height: 3),
              Text(subtitle, style: Theme.of(context).textTheme.bodySmall?.copyWith(color: Colors.white60)),
            ],
          ),
        ),
      );
}
