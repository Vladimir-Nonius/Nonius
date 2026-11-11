# Сторож: держит SolidWorks запущенным, пока он нужен Nonius, и закрывает
# его, когда он полминуты не нужен.
#
# Зачем. Запуск SolidWorks — 15–20 секунд. Невидимый SolidWorks, запущенный
# по команде программы, сам завершается, как только эта программа от него
# отключилась. Скрипты выгрузки короткие: без сторожа разбор папки из сорока
# деталей запускал бы SolidWorks сорок раз. Сторож подключается к нему
# первым и не отключается, пока идёт работа, — остальные скрипты приходят
# уже к запущенному.
#
# -Stamp — файл-отметка: Nonius обновляет его при каждом обращении к
# SolidWorks. Сторож уходит, когда отметке больше $Idle секунд.
#
# Закрывается только «наш» SolidWorks: невидимый, не управляемый человеком
# и без открытых документов. Если SolidWorks открыт человеком, сторож
# подключается к нему же, ничего в нём не трогает и в конце просто уходит.
param(
    [Parameter(Mandatory = $true)][string]$Stamp,
    [int]$Idle = 30
)

$Flags = [Reflection.BindingFlags]

# Сторож нужен один: второй сразу уходит.
$created = $false
$mutex = New-Object System.Threading.Mutex($true, 'NoniusSolidWorksKeeper', [ref]$created)
if (-not $created) { exit 0 }

try {
    try {
        $sw = New-Object -ComObject SldWorks.Application
    } catch {
        exit 0
    }
    while ($true) {
        if (-not (Test-Path -LiteralPath $Stamp)) { break }
        $quiet = ((Get-Date) - (Get-Item -LiteralPath $Stamp).LastWriteTime).TotalSeconds
        if ($quiet -ge $Idle) { break }
        Start-Sleep -Seconds ([Math]::Max(1, [int]($Idle - $quiet)))
    }
    $visible = [System.__ComObject].InvokeMember('Visible', $Flags::GetProperty, $null, $sw, $null)
    $user = [System.__ComObject].InvokeMember('UserControl', $Flags::GetProperty, $null, $sw, $null)
    $docs = [System.__ComObject].InvokeMember('GetDocumentCount', $Flags::InvokeMethod, $null, $sw, $null)
    if (-not $visible -and -not $user -and $docs -eq 0) {
        $id = [System.__ComObject].InvokeMember('GetProcessID', $Flags::InvokeMethod, $null, $sw, $null)
        [System.__ComObject].InvokeMember('ExitApp', $Flags::InvokeMethod, $null, $sw, $null) | Out-Null
        [Runtime.InteropServices.Marshal]::ReleaseComObject($sw) | Out-Null
        # Бывает, что после «закройся» SolidWorks перестаёт отвечать, а
        # процесс остаётся висеть (полгигабайта памяти). Ждём 15 секунд и
        # снимаем его: это наш невидимый SolidWorks без документов, терять
        # в нём нечего.
        $process = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($process -and $process.ProcessName -eq 'SLDWORKS') {
            if (-not $process.WaitForExit(15000)) {
                Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
            }
        }
    }
} finally {
    $mutex.ReleaseMutex()
}
