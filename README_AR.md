# Frame Sense – Starter Kit

هذه الحزمة هي **بداية مشروع فعلية قابلة للفتح في VS Code**، وليست ادعاءً بأن المنتج النهائي اكتمل.

## ماذا تحتوي؟

- `desktop/`: أساس Desktop باستخدام Tauri 2 + Rust + React + TypeScript.
- `mobile/`: أساس Flutter مستقل للموبايل مع `pubspec.yaml` وواجهة أولية.
- `docs/`: المواصفات الأصلية V1.1، سجل القرارات، خارطة الطريق، الاختبارات، ومراجع الواجهات.
- `shared/`: عقود بيانات أولية وهيكل مجلدات التسليم.
- `.vscode/`: إعدادات واقتراحات إضافات VS Code.
- `FrameSense.code-workspace`: ملف فتح مساحة العمل كاملة.

## افتح المشروع

1. فك ضغط `FrameSense_Starter_Kit.zip`.
2. افتح VS Code.
3. اختر **File → Open Folder**.
4. اختر مجلد `FrameSense_Starter_Kit`.
5. أو افتح ملف `FrameSense.code-workspace`.

## الأدوات المطلوبة على Windows

المثبتة بحسب ما ظهر في محادثتنا:
- Rust + Cargo.
- Microsoft C++ Build Tools مع MSVC x64 وWindows SDK.
- Node.js + npm.
- Git.
- Android Studio (تم تثبيته، لكن لم نتحقق بعد من كل Android SDK components).

تحتاج إلى التحقق/الإكمال:
- Tauri CLI.
- Flutter SDK نفسه (تثبيت Android Studio لا يعني أن Flutter SDK مثبت).
- Android SDK / Platform Tools / Build Tools / Command-line Tools / NDK / CMake.
- Flutter وDart plugins داخل Android Studio أو إضافات VS Code.

## تثبيت Tauri CLI

افتح Developer Command Prompt أو PowerShell جديداً بعد اكتمال Rust، ثم نفّذ:

```powershell
cargo install tauri-cli --version "^2.0.0" --locked
```

تحقق:

```powershell
cargo tauri --version
```

## تشغيل Desktop scaffold

من Terminal داخل VS Code:

```powershell
cd desktop
npm install
cargo tauri dev
```

لإنشاء نسخة Windows:

```powershell
cargo tauri build
```

إذا لم يتعرف Windows على `cargo tauri`، أغلق Terminal وافتحه من جديد بعد تثبيت CLI، وتأكد أن مجلد Cargo bin موجود في PATH:

```text
%USERPROFILE%\.cargo\bin
```

## تجهيز Flutter

1. نزّل Flutter SDK Stable من: https://docs.flutter.dev/install/manual
2. فكّه مثلاً في `C:\src\flutter`.
3. أضف `C:\src\flutter\bin` إلى PATH.
4. افتح Terminal جديداً واختبر:

```powershell
flutter --version
dart --version
flutter doctor
```

5. من Android Studio افتح SDK Manager وثبّت Android SDK Platform وPlatform-Tools وBuild-Tools وCommand-line Tools وNDK وCMake.
6. وافق على التراخيص:

```powershell
flutter doctor --android-licenses
```

7. افتح مجلد `mobile` في Terminal:

```powershell
cd mobile
flutter pub get
flutter create --platforms=android,ios --project-name framesense_mobile .
flutter run
```

إذا رفض Flutter إنشاء ملفات المنصات داخل مجلد غير فارغ، أنشئ مشروعاً مؤقتاً جديداً بالأمر التالي، ثم انقل مجلدي `android` و`ios` الناتجين إلى `mobile`:

```powershell
flutter create --platforms=android,ios --project-name framesense_mobile mobile_platforms_seed
```

**ملاحظة iOS/macOS:** بناء تطبيق iOS أو macOS النهائي يتطلب جهاز Mac وXcode. يمكن تجهيز كود Flutter على Windows، لكن لا يمكن إنتاج بناء iOS رسمي من Windows وحده.

## ما الذي يعمل في هذه المرحلة؟

- هيكل Tauri 2 + React + TypeScript.
- واجهة Desktop أولية بثلاث مناطق وثيمين قابلين للاختيار.
- أمر إنشاء مشروع محلي ينشئ المجلدات الأساسية وقاعدة SQLite.
- أمر استيراد أولي ينسخ ملفات الصور المدعومة كملفات bytes، يحافظ على المصدر، يتحقق من SHA-256، ويتعامل مع تكرار الملف داخل الكاميرا نفسها وتصادم الاسم.
- هيكل Flutter أولي للتنقل بين Home وProjects وGallery وEdit وDevices.
- توثيق المتطلبات والقرارات ومراجع الواجهات.

## ما الذي لم يُنفذ بعد؟

- لا يوجد RAW demosaicing أو تطوير RAW أو محرك ألوان فعلي.
- لا توجد نماذج AI مدمجة أو Face Retouch أو Focus/Noise/Flash AI منفذة.
- لا يوجد اكتشاف تلقائي فعلي لقارئ الكروت بمجرد توصيله.
- لا يوجد Job Queue كامل أو معالجة فعلية للأوردرات.
- لا يوجد تصدير JPEG احترافي أو ICC pipeline.
- لا يوجد Remote Gallery أو QR pairing أو تشفير نقل فعلي.
- لا يوجد Google/Email authentication أو مزامنة حساب.
- لا يوجد بناء macOS/iOS تم اختباره.
- واجهات الأقسام غير المنفذة تعرض حالة Planned ولا تدّعي أنها تعمل.

## قاعدة العمل

المواصفات في `docs/FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.docx` هي المرجع. لا تغيّر قراراً معتمداً. لا تعتبر أي زر أو شاشة دليلاً على اكتمال الوظيفة. سجّل لكل ميزة: Implemented & Tested / Partial / Planned / Blocked.
