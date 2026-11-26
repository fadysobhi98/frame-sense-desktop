Write-Host "Frame Sense - Local Environment Check" -ForegroundColor Cyan
$commands = @("git", "node", "npm", "rustc", "cargo", "flutter", "dart")
foreach ($name in $commands) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) {
    Write-Host ("FOUND  {0} -> {1}" -f $name, $cmd.Source) -ForegroundColor Green
    try { & $name --version 2>&1 | Select-Object -First 1 } catch {}
  } else {
    Write-Host ("MISSING {0}" -f $name) -ForegroundColor Yellow
  }
}
Write-Host ""
Write-Host "MSVC check: open an x64 Native Tools / Developer Command Prompt, then run: cl" -ForegroundColor Gray
Write-Host "Flutter check: run flutter doctor after installing Flutter SDK and Android SDK." -ForegroundColor Gray
