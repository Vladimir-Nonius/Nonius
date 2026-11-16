# Рабочий процесс Nonius для SolidWorks: один на всю работу программы.
#
# Запускает СВОЙ экземпляр SolidWorks и держит его COM-объект. К SolidWorks,
# открытому человеком, не подключается и не трогает его: там несохранённая
# работа, а трудная деталь может SolidWorks уронить.
#
# Как получается «свой» (docs/formats/solidworks-automation.md, §2):
#   1. сами запускаем SLDWORKS.exe /r (без заставки и сообщений);
#   2. берём COM-объект именно этого процесса из таблицы запущенных объектов
#      Windows по имени SolidWorks_PID_<номер процесса> (helper.cs);
#   3. ждём, пока он скажет, что запуск окончен (StartupProcessCompleted);
#   4. Visible = false, UserControl = false — свойства только этого сеанса.
# Свой SolidWorks живёт в группе процессов (Job Object): не стало рабочего
# процесса или самого Nonius — Windows закрывает и его.
#
# Чего здесь нет и быть не должно — это общие настройки человека:
# SetUserPreference*, параметры STEP, надстройки (AddInsStartup), OpenGL,
# FrameState. STEP пишется по тем настройкам, что у человека.
#
# Разговор с Nonius (crates/nonius-solidworks/src/wire.rs) — строками UTF-8,
# поля через табуляцию. Команды из stdin:
#   step<таб>файл<таб>куда   — выгрузить STEP
#   deps<таб>файл            — на какие файлы ссылается (сборка, чертёж)
#   ping                     — жив ли SolidWorks
#   quit                     — закрыть свой SolidWorks и уйти (то же — конец stdin)
# Ответы в stdout:
#   READY<таб>pid<таб>время старта<таб>версия — свой SolidWorks готов
#   DEP<таб>путь                              — строка ответа на deps
#   OK                                        — команда выполнена
#   ERROR<таб>код<таб>причина                 — не выполнена; код — ошибка COM
#                                               (0x800706BA…), 1 — SolidWorks
#                                               жив и отказал окончательно
#                                               (файл не открывается), 0 — прочее
#   BEAT<таб>мс процессора<таб>окно, GONE     — пульс сторожевой нити
#
# -Dry — без SolidWorks вовсе (для тестов Nonius): step копирует файл,
# deps отвечает самим файлом.
#
# Почему вызовы идут через InvokeMember, а не просто `$sw.OpenDoc6(...)`:
# PowerShell для короткой записи читает описание типов SolidWorks из реестра,
# а оно часто битое (остатки старых версий) — тогда любой вызов кончается
# ошибкой TYPE_E_ELEMENTNOTFOUND. InvokeMember зовёт по имени, без описания.
param(
    [Parameter(Mandatory = $true)][string]$Helper,
    [string]$Exe = '',
    [int]$Parent = 0,
    [switch]$Dry
)

$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 по умолчанию читает и пишет не в UTF-8.
try {
    [Console]::InputEncoding = New-Object System.Text.UTF8Encoding $false
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
} catch { }
Add-Type -Path $Helper
[Nonius.Wire]::Open()
$Flags = [Reflection.BindingFlags]
$script:sw = $null

function Say([string]$line) { [Nonius.Wire]::Say($line) }

function Call($object, [string]$name, $arguments = @()) {
    [System.__ComObject].InvokeMember($name, $Flags::InvokeMethod, $null, $object, $arguments)
}

function Read($object, [string]$name) {
    [System.__ComObject].InvokeMember($name, $Flags::GetProperty, $null, $object, $null)
}

# (Не `Write`: так в PowerShell зовётся Write-Output.)
function Put($object, [string]$name, $value) {
    [System.__ComObject].InvokeMember($name, $Flags::SetProperty, $null, $object, @($value)) | Out-Null
}

# Пометка в начале причины: отказ окончательный — пока файл тот же,
# повторять бессмысленно.
$Final = 'FINAL:'

# Код COM-ошибки (0x80010108 — «SolidWorks умер» и т. п.); 1 — окончательный
# отказ; 0 — прочее.
function Code($problem) {
    for ($e = $problem; $null -ne $e; $e = $e.InnerException) {
        if ($e -is [Runtime.InteropServices.COMException]) {
            return ('0x{0:X8}' -f $e.ErrorCode)
        }
    }
    $e = $problem
    while ($null -ne $e.InnerException) { $e = $e.InnerException }
    if (([string]$e.Message).StartsWith($Final)) { return '1' }
    return '0'
}

function Text($problem) {
    $e = $problem
    while ($null -ne $e.InnerException) { $e = $e.InnerException }
    $text = ([string]$e.Message).Replace("`t", ' ')
    if ($text.StartsWith($Final)) { $text = $text.Substring($Final.Length) }
    return $text
}

function Start-Own {
    $fenced = $false
    $id = [Nonius.Own]::Start($Exe, [ref]$fenced)
    $deadline = (Get-Date).AddSeconds(170)
    while ($null -eq $script:sw) {
        if ([Nonius.Own]::Exited()) { throw 'SolidWorks закрылся, не успев запуститься' }
        if ((Get-Date) -gt $deadline) { throw 'SolidWorks слишком долго запускается' }
        Start-Sleep -Milliseconds 300
        $script:sw = [Nonius.Own]::Find($id)
    }
    while (-not (Read $script:sw 'StartupProcessCompleted')) {
        if ((Get-Date) -gt $deadline) { throw 'SolidWorks слишком долго запускается' }
        Start-Sleep -Milliseconds 300
    }
    # Сверка: объект из таблицы — именно наш процесс.
    if ((Call $script:sw 'GetProcessID') -ne $id) { throw 'это не наш SolidWorks' }
    Put $script:sw 'Visible' $false
    Put $script:sw 'UserControl' $false
    $revision = Read $script:sw 'RevisionNumber'
    $group = if ($fenced) { 'job' } else { 'nojob' }
    Say "READY`t$id`t$([Nonius.Own]::Started())`t$revision`t$group"
}

function Export([string]$Source, [string]$Target) {
    if ($Dry) {
        Copy-Item -LiteralPath $Source -Destination $Target
        return
    }
    # swDocPART = 1, swDocASSEMBLY = 2
    $kind = if ($Source -match '\.sldasm$') { 2 } else { 1 }
    # swOpenDocOptions_Silent (1) + swOpenDocOptions_ReadOnly (2);
    # два последних значения SolidWorks заполняет сам: ошибка и замечания.
    # Деталь открывается с окном: без окна детали в невидимом SolidWorks
    # падает его модуль выгрузки STEP (проверено на 2025).
    $arguments = @($Source, $kind, 3, '', 0, 0)
    $byRef = New-Object Reflection.ParameterModifier 6
    $byRef[4] = $true
    $byRef[5] = $true
    $doc = [System.__ComObject].InvokeMember(
        'OpenDoc6', $Flags::InvokeMethod, $null, $script:sw, $arguments, @($byRef), $null, $null)
    if ($null -eq $doc) { throw "${Final}SolidWorks не открыл файл (код $($arguments[4]))" }
    try {
        # swSaveAsCurrentVersion = 0, swSaveAsOptions_Silent (1) + Copy (2):
        # «копия» — чтобы открытая деталь не стала считать себя файлом STEP.
        # Ответ — код; он бывает не нулевым и при удаче (64 — «в детали есть
        # ошибки перестроения», файл при этом записан), поэтому судим по файлу.
        if (Test-Path -LiteralPath $Target) { Remove-Item -LiteralPath $Target }
        $code = Call $doc 'SaveAs3' @($Target, 0, 3)
        if (-not (Test-Path -LiteralPath $Target)) {
            throw "SolidWorks не выгрузил STEP (код $code)"
        }
    } finally {
        # Закрыть всё, что открылось (у сборки — и её детали). Умер
        # SolidWorks — закрывать нечего, главное не потерять первую ошибку.
        try { Call $script:sw 'CloseAllDocuments' @($true) | Out-Null } catch { }
    }
}

function Deps([string]$File) {
    if ($Dry) {
        Say "DEP`t$File"
        return
    }
    # Только прямые ссылки (без вложенных), без поиска по папкам SolidWorks,
    # без пометок «только чтение». SolidWorks отвечает по файлу на диске, не
    # открывая его. Ответ — пары «имя, путь» подряд; у файла без ссылок
    # ответа нет.
    $pairs = @(Call $script:sw 'GetDocumentDependencies2' @($File, $false, $false, $false))
    for ($i = 1; $i -lt $pairs.Count; $i += 2) {
        if ($pairs[$i]) { Say "DEP`t$($pairs[$i])" }
    }
}

# Всё, что функции нечаянно «вернули», гасится: в stdout идут только
# строки Say.
& {
    try {
        if ($Parent -gt 0) { [Nonius.Own]::Watch($Parent) }
        if ($Dry) { Say "READY`t0`t0`tdry`tnojob" } else { Start-Own }
    } catch {
        Say "ERROR`t$(Code $_.Exception)`t$(Text $_.Exception)"
        [Nonius.Own]::Kill()
        exit 1
    }
    $in = [Nonius.Wire]::Input()
    while ($true) {
        $line = $in.ReadLine()
        if ($null -eq $line) { break }
        $cells = $line.Split("`t")
        if ($cells[0] -eq 'quit') { break }
        try {
            switch ($cells[0]) {
                'ping' { if (-not $Dry) { Read $script:sw 'RevisionNumber' | Out-Null } }
                'step' { Export $cells[1] $cells[2] }
                'deps' { Deps $cells[1] }
                default { throw "неизвестная команда: $($cells[0])" }
            }
            Say 'OK'
        } catch {
            Say "ERROR`t$(Code $_.Exception)`t$(Text $_.Exception)"
        }
    }
    # Вежливо закрыть свой SolidWorks; не ушёл за 15 секунд — снять: это наш
    # невидимый экземпляр, терять в нём нечего.
    if (-not $Dry) {
        try { Call $script:sw 'ExitApp' | Out-Null } catch { }
        if (-not [Nonius.Own]::WaitExit(15000)) { [Nonius.Own]::Kill() }
    }
} | Out-Null
