# Выгрузить STEP из файла SolidWorks (.SLDPRT, .SLDASM) самим SolidWorks.
#
# Запускает Nonius (crates/nonius-solidworks/src/lib.rs). Ответ — последняя
# строка вывода: "OK" или "ERROR: причина по-русски".
#
# Правила вежливости к человеку, который в это время работает в SolidWorks:
#   - SolidWorks уже открыт — работаем в нём, но ничего не закрываем и не
#     трогаем настройки; деталь открываем без окна;
#   - деталь уже открыта — выгружаем её и оставляем открытой; если в ней
#     есть несохранённые правки — отказываемся: STEP не совпал бы с файлом;
#   - SolidWorks запустили мы — он невидим (так он запускается по команде
#     программы сам, мы видимость не трогаем), и мы же его закрываем — но
#     не сразу, а когда он полминуты не нужен (close.ps1): следующей детали
#     не придётся ждать нового запуска.
#
# Как отличить «наш» SolidWorks от открытого человеком: наш невидим, им не
# управляет человек (UserControl) и в нём нет открытых документов. Только
# такой и закрываем. Список процессов для этого не годится: закрытый
# SolidWorks ещё с минуту висит в нём, пока завершается.
#
# Только что запущенный SolidWorks ещё догружает свои части, поэтому ждём,
# пока он сам скажет, что запуск окончен (StartupProcessCompleted).
#
# Почему вызовы идут через InvokeMember, а не просто `$sw.OpenDoc6(...)`:
# PowerShell для короткой записи читает описание типов SolidWorks из реестра,
# а оно часто битое (остатки старых версий) — тогда любой вызов кончается
# ошибкой TYPE_E_ELEMENTNOTFOUND. InvokeMember зовёт по имени, без описания.
param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Target
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$Flags = [Reflection.BindingFlags]

function Fail([string]$why) {
    Write-Output "ERROR: $why"
    exit 1
}

function Call($object, [string]$name, $arguments = @()) {
    [System.__ComObject].InvokeMember($name, $Flags::InvokeMethod, $null, $object, $arguments)
}

function Read($object, [string]$name) {
    [System.__ComObject].InvokeMember($name, $Flags::GetProperty, $null, $object, $null)
}

try {
    $sw = New-Object -ComObject SldWorks.Application
} catch {
    Fail 'SolidWorks не установлен или не запускается'
}

$opened = $false
$hidden = $false
# swDocPART = 1, swDocASSEMBLY = 2
$kind = if ($Source -match '\.sldasm$') { 2 } else { 1 }
try {
    # Ждём конца запуска, но не дольше двух минут.
    $deadline = (Get-Date).AddSeconds(120)
    while (-not (Read $sw 'StartupProcessCompleted')) {
        if ((Get-Date) -gt $deadline) {
            Fail 'SolidWorks слишком долго запускается'
        }
        Start-Sleep -Milliseconds 500
    }

    $doc = Call $sw 'GetOpenDocumentByName' @($Source)
    if ($null -ne $doc) {
        if (Call $doc 'GetSaveFlag') {
            Fail 'деталь открыта в SolidWorks с несохранёнными правками: сохраните её и повторите'
        }
    } else {
        # В открытом SolidWorks — открыть деталь без окна, чтобы у человека
        # ничего не мелькало. В невидимом так делать нельзя: без окна детали
        # и без окна программы падает модуль выгрузки STEP (проверено на 2025).
        if (Read $sw 'Visible') {
            Call $sw 'DocumentVisible' @($false, $kind) | Out-Null
            $hidden = $true
        }
        # swOpenDocOptions_Silent (1) + swOpenDocOptions_ReadOnly (2);
        # два последних значения SolidWorks заполняет сам: ошибка и замечания.
        $arguments = @($Source, $kind, 3, '', 0, 0)
        $byRef = New-Object Reflection.ParameterModifier 6
        $byRef[4] = $true
        $byRef[5] = $true
        $doc = [System.__ComObject].InvokeMember(
            'OpenDoc6', $Flags::InvokeMethod, $null, $sw, $arguments, @($byRef), $null, $null)
        if ($null -eq $doc) {
            Fail "SolidWorks не открыл файл (код $($arguments[4]))"
        }
        $opened = $true
    }

    # swSaveAsCurrentVersion = 0, swSaveAsOptions_Silent (1) + Copy (2):
    # «копия» — чтобы открытая деталь не стала считать себя файлом STEP.
    # Ответ — код; он бывает не нулевым и при удаче (64 — «в детали есть
    # ошибки перестроения», файл при этом записан), поэтому судим по файлу.
    if (Test-Path -LiteralPath $Target) {
        Remove-Item -LiteralPath $Target
    }
    $code = Call $doc 'SaveAs3' @($Target, 0, 3)
    if (-not (Test-Path -LiteralPath $Target)) {
        Fail "SolidWorks не выгрузил STEP (код $code)"
    }
    Write-Output 'OK'
} catch {
    Fail "SolidWorks: $($_.Exception.Message)"
} finally {
    # Вывод глушим: последней строкой должен остаться ответ.
    if ($opened) {
        Call $sw 'CloseDoc' @((Call $doc 'GetTitle')) | Out-Null
    }
    if ($hidden) {
        Call $sw 'DocumentVisible' @($true, $kind) | Out-Null
    }
}
