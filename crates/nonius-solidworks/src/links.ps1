# Спросить у SolidWorks связи файлов: из каких деталей собрана сборка,
# к какой модели относится чертёж.
#
# Запускает Nonius (crates/nonius-solidworks/src/links.rs). -List — текстовый
# файл (UTF-8), в каждой строке полный путь файла .SLDASM / .SLDDRW / .SLDPRT.
# Ответ строками:
#   FILE<таб>путь          — про какой файл дальше речь
#   DEP<таб>путь           — файл, на который он ссылается напрямую
#   OK  или  ERROR: причина — последней строкой
#
# SolidWorks отвечает по самому файлу на диске, не открывая его: это быстро
# (миллисекунды на файл) и ничего не меняет ни в файлах, ни в открытом окне.
# Правила вежливости те же, что в export_step.ps1: открытый человеком
# SolidWorks не закрываем, свой невидимый закрывает close.ps1.
param(
    [Parameter(Mandatory = $true)][string]$List
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

try {
    $deadline = (Get-Date).AddSeconds(120)
    while (-not (Read $sw 'StartupProcessCompleted')) {
        if ((Get-Date) -gt $deadline) {
            Fail 'SolidWorks слишком долго запускается'
        }
        Start-Sleep -Milliseconds 500
    }

    foreach ($line in [System.IO.File]::ReadAllLines($List, [System.Text.Encoding]::UTF8)) {
        [string]$file = $line.Trim()
        if ($file -eq '') { continue }
        Write-Output "FILE`t$file"
        try {
            # Только прямые ссылки (без вложенных), без поиска по папкам
            # SolidWorks, без пометок «только чтение». Ответ — пары
            # «имя, путь» подряд; у файла без ссылок ответа нет.
            $pairs = @(Call $sw 'GetDocumentDependencies2' @($file, $false, $false, $false))
            for ($i = 1; $i -lt $pairs.Count; $i += 2) {
                if ($pairs[$i]) { Write-Output "DEP`t$($pairs[$i])" }
            }
        } catch {
            # Один нечитаемый файл не должен ронять разбор всей папки.
        }
    }
    Write-Output 'OK'
} catch {
    Fail "SolidWorks: $($_.Exception.Message)"
}
