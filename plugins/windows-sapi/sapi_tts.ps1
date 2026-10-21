# Плагин синтеза речи Звукопада: голоса Windows (SAPI 5) через System.Speech.
# Работает в Windows PowerShell 5.1, который есть в любой Windows 10/11.
# Протокол: одна строка JSON на запрос и одна на ответ (см. docs/PLUGINS.md).

$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding $false
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
Add-Type -AssemblyName System.Speech

# Все не-ASCII символы отправляем как \uXXXX — так кодировка консоли не важна.
function Send-Json($obj) {
    $json = $obj | ConvertTo-Json -Compress -Depth 6
    $json = [regex]::Replace($json, '[^\x00-\x7F]', { param($m) '\u{0:x4}' -f [int][char]$m.Value })
    [Console]::Out.WriteLine($json)
    [Console]::Out.Flush()
}

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer

while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    if ($line.Trim() -eq '') { continue }
    $req = $null
    try {
        $req = $line | ConvertFrom-Json
        switch ([string]$req.method) {
            'info' {
                Send-Json @{ id = $req.id; result = @{ name = 'Windows SAPI'; protocol = 1 } }
            }
            'list_voices' {
                $voices = @()
                foreach ($v in $synth.GetInstalledVoices()) {
                    if (-not $v.Enabled) { continue }
                    $i = $v.VoiceInfo
                    $voices += @{ id = $i.Name; name = $i.Name; language = $i.Culture.Name }
                }
                Send-Json @{ id = $req.id; result = @{ voices = $voices } }
            }
            'synthesize' {
                $p = $req.params
                if ($p.voice) { $synth.SelectVoice([string]$p.voice) }
                $rate = [int]$p.rate
                if ($rate -lt -10) { $rate = -10 }
                if ($rate -gt 10) { $rate = 10 }
                $synth.Rate = $rate
                $synth.Volume = 100
                $synth.SetOutputToWaveFile([string]$p.output)
                try { $synth.Speak([string]$p.text) } finally { $synth.SetOutputToNull() }
                Send-Json @{ id = $req.id; result = @{ path = [string]$p.output } }
            }
            'shutdown' {
                $synth.Dispose()
                exit 0
            }
            default {
                Send-Json @{ id = $req.id; error = @{ message = "неизвестный метод: $($req.method)" } }
            }
        }
    }
    catch {
        $rid = 0
        if ($req -and $req.id) { $rid = $req.id }
        Send-Json @{ id = $rid; error = @{ message = $_.Exception.Message } }
    }
}
