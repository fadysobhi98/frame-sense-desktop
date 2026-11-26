# Mobile – Flutter foundation

This folder contains a minimal Flutter source scaffold and `pubspec.yaml`.

## Important
Android Studio installation does not prove Flutter SDK is installed. Install Flutter SDK separately, add `flutter\bin` to PATH, then run:

```powershell
flutter --version
dart --version
flutter doctor
```

Install Flutter and Dart plugins in Android Studio/VS Code after the SDK is available.

Then:

```powershell
flutter pub get
flutter analyze
```

Generate native platform folders if missing:

```powershell
flutter create --platforms=android,ios --project-name framesense_mobile .
```

A Windows machine can develop and build Android. An iOS build/signing requires macOS and Xcode.
