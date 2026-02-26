# test_mcp.ps1 — aimcp MCP 协议端到端测试
# 用法: powershell -ExecutionPolicy Bypass -File test_mcp.ps1
#
# 前置条件:
#   $env:GROK_API_URL = "https://api.x.ai/v1"
#   $env:GROK_API_KEY = "xai-..."
#   $env:GROK_DEBUG   = "true"  (可选, 推荐)

param(
    [switch]$SkipSearch,     # 跳过耗时的搜索/抓取测试
    [switch]$SkipCli,        # 跳过 gemini/codex CLI 工具测试（需要对应 CLI 已安装）
    [switch]$TestTimeout     # 用短超时验证超时机制
)

$ErrorActionPreference = "Stop"
$passed = 0
$failed = 0
$skipped = 0

function Write-TestHeader($name) {
    Write-Host "`n========================================" -ForegroundColor DarkGray
    Write-Host "  TEST: $name" -ForegroundColor White
    Write-Host "========================================" -ForegroundColor DarkGray
}

function Write-Pass($msg) {
    $script:passed++
    Write-Host "  [PASS] $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    $script:failed++
    Write-Host "  [FAIL] $msg" -ForegroundColor Red
}

function Write-Skip($msg) {
    $script:skipped++
    Write-Host "  [SKIP] $msg" -ForegroundColor Yellow
}

# ─── Helper: start aimcp process and exchange JSON-RPC messages ───

function Start-McpSession {
    param(
        [hashtable]$ExtraEnv = @{}
    )

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "aimcp"
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true

    # Inherit current env
    foreach ($key in @("GROK_API_URL","GROK_API_KEY","GROK_DEBUG","GROK_MODEL",
                       "GROK_IDLE_TIMEOUT","GROK_STREAM_TIMEOUT","GROK_TOTAL_TIMEOUT",
                       "GEMINI_API_KEY","GEMINI_IMAGE_API_KEY","GEMINI_API_URL",
                       "GEMINI_FORCE_MODEL","GEMINI_IMAGE_MODEL","GEMINI_BIN",
                       "CODEX_BIN")) {
        $val = [System.Environment]::GetEnvironmentVariable($key)
        if ($val) { $psi.Environment[$key] = $val }
    }

    # Apply extra env overrides
    foreach ($kv in $ExtraEnv.GetEnumerator()) {
        $psi.Environment[$kv.Key] = $kv.Value
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    return $proc
}

function Send-JsonRpc($proc, $request, $timeoutMs = 30000) {
    $proc.StandardInput.WriteLine($request)
    $proc.StandardInput.Flush()

    # Read with timeout using async
    $task = $proc.StandardOutput.ReadLineAsync()
    if ($task.Wait($timeoutMs)) {
        return $task.Result
    } else {
        return $null
    }
}

function Stop-McpSession($proc) {
    try {
        $proc.StandardInput.Close()
    } catch {}
    if (!$proc.WaitForExit(3000)) {
        $proc.Kill()
    }
    try {
        $stderr = $proc.StandardError.ReadToEnd()
        return $stderr
    } catch {
        return ""
    }
}

# ─── JSON-RPC message templates ───

$initRequest = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-script","version":"1.0"}}}'
$listToolsRequest = '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
$configRequest = '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_config_info","arguments":{}}}'
$searchRequest = '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"web_search","arguments":{"query":"Rust programming language","min_results":3,"max_results":5}}}'
$searchTimeRequest = '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"web_search","arguments":{"query":"latest Rust release","min_results":3,"max_results":5}}}'
$fetchRequest = '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"web_fetch","arguments":{"url":"https://httpbin.org/html"}}}'
$emptyQueryRequest = '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"web_search","arguments":{"query":"","min_results":3,"max_results":10}}}'
$emptyUrlRequest = '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"web_fetch","arguments":{"url":""}}}'

# Gemini / Codex requests
$geminiRequest = '{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"gemini","arguments":{"PROMPT":"What is 2+2? Reply with just the number.","timeout_secs":30}}}'
$geminiImageRequest = '{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"gemini_image","arguments":{"PROMPT":"Generate a simple red circle on white background","timeout_secs":60}}}'
$geminiEmptyPrompt = '{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"gemini","arguments":{"PROMPT":"","timeout_secs":10}}}'
$codexRequest = '{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"codex","arguments":{"PROMPT":"What is 2+2? Reply with just the number.","cd":".","image":[],"timeout_secs":30}}}'
$codexEmptyPrompt = '{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"codex","arguments":{"PROMPT":"","cd":".","image":[]}}}'

# ═══════════════════════════════════════════════════════════════════
#  TEST 1: CLI 冒烟测试
# ═══════════════════════════════════════════════════════════════════

Write-TestHeader "CLI --help"
$helpOutput = & aimcp --help 2>&1 | Out-String
if ($helpOutput -match "aimcp|Unified AI MCP Server") {
    Write-Pass "--help output contains expected text"
} else {
    Write-Fail "--help output missing expected text: $helpOutput"
}

Write-TestHeader "CLI --version"
$versionOutput = & aimcp --version 2>&1 | Out-String
if ($versionOutput -match "aimcp") {
    Write-Pass "--version output contains 'aimcp'"
} else {
    Write-Fail "--version output missing 'aimcp': $versionOutput"
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 2: MCP initialize + tools/list
# ═══════════════════════════════════════════════════════════════════

Write-TestHeader "MCP initialize + tools/list"

$proc = Start-McpSession
$initResponse = Send-JsonRpc $proc $initRequest

if ($initResponse -and $initResponse -match '"result"') {
    Write-Pass "initialize response is valid JSON-RPC"
} else {
    Write-Fail "initialize failed: $initResponse"
}

$toolsResponse = Send-JsonRpc $proc $listToolsRequest

if ($toolsResponse -and $toolsResponse -match '"tools"') {
    Write-Pass "tools/list returned tool list"

    # Check for specific tools
    foreach ($tool in @("web_search", "web_fetch", "get_config_info")) {
        if ($toolsResponse -match "`"$tool`"") {
            Write-Pass "  Found tool: $tool"
        } else {
            Write-Fail "  Missing tool: $tool"
        }
    }
} else {
    Write-Fail "tools/list failed: $toolsResponse"
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 3: get_config_info
# ═══════════════════════════════════════════════════════════════════

Write-TestHeader "get_config_info"

$configResponse = Send-JsonRpc $proc $configRequest 10000

if ($configResponse -and $configResponse -match 'config_status') {
    Write-Pass "get_config_info returned config data"
    if ($configResponse -match 'Connected') {
        Write-Pass "  API connection test: Connected"
    } else {
        Write-Fail "  API connection test: not connected"
    }
} else {
    Write-Fail "get_config_info failed: $configResponse"
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 4: web_search (核心 — 验证卡死修复)
# ═══════════════════════════════════════════════════════════════════

if ($SkipSearch) {
    Write-TestHeader "web_search (SKIPPED)"
    Write-Skip "web_search skipped via -SkipSearch flag"
} else {
    Write-TestHeader "web_search — 普通查询"

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $searchResponse = Send-JsonRpc $proc $searchRequest 60000
    $sw.Stop()

    if ($searchResponse -and $searchResponse -match '"result"') {
        Write-Pass "web_search returned result in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
        if ($searchResponse -match 'title') {
            Write-Pass "  Result contains search results with titles"
        }
    } elseif ($null -eq $searchResponse) {
        Write-Fail "web_search TIMEOUT (60s) — 卡死未修复!"
    } else {
        Write-Fail "web_search error: $searchResponse"
    }

    Write-TestHeader "web_search — 时间上下文查询"

    $sw.Restart()
    $searchTimeResponse = Send-JsonRpc $proc $searchTimeRequest 60000
    $sw.Stop()

    if ($searchTimeResponse -and $searchTimeResponse -match '"result"') {
        Write-Pass "web_search (time context) returned in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
    } elseif ($null -eq $searchTimeResponse) {
        Write-Fail "web_search (time context) TIMEOUT (60s)"
    } else {
        Write-Fail "web_search (time context) error: $searchTimeResponse"
    }
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 5: web_fetch
# ═══════════════════════════════════════════════════════════════════

if ($SkipSearch) {
    Write-TestHeader "web_fetch (SKIPPED)"
    Write-Skip "web_fetch skipped via -SkipSearch flag"
} else {
    Write-TestHeader "web_fetch"

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $fetchResponse = Send-JsonRpc $proc $fetchRequest 60000
    $sw.Stop()

    if ($fetchResponse -and $fetchResponse -match '"result"') {
        Write-Pass "web_fetch returned result in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
    } elseif ($null -eq $fetchResponse) {
        Write-Fail "web_fetch TIMEOUT (60s)"
    } else {
        Write-Fail "web_fetch error: $fetchResponse"
    }
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 6: Gemini CLI
# ═══════════════════════════════════════════════════════════════════

if ($SkipCli) {
    Write-TestHeader "gemini (SKIPPED)"
    Write-Skip "gemini skipped via -SkipCli flag"
} else {
    # Check if gemini tool is in the tools list
    if ($toolsResponse -and $toolsResponse -match '"gemini"') {
        Write-TestHeader "gemini — 简单问答"

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $geminiResponse = Send-JsonRpc $proc $geminiRequest 60000
        $sw.Stop()

        if ($geminiResponse -and $geminiResponse -match '"result"') {
            Write-Pass "gemini returned result in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
            if ($geminiResponse -match 'SESSION_ID|session_id|agent_messages') {
                Write-Pass "  Response contains expected fields (SESSION_ID/agent_messages)"
            }
            if ($geminiResponse -match '4') {
                Write-Pass "  Response contains correct answer (4)"
            }
        } elseif ($null -eq $geminiResponse) {
            Write-Fail "gemini TIMEOUT (60s)"
        } else {
            Write-Fail "gemini error: $($geminiResponse.Substring(0, [Math]::Min(200, $geminiResponse.Length)))"
        }

        Write-TestHeader "gemini — 空 prompt 错误处理"

        $geminiEmptyResponse = Send-JsonRpc $proc $geminiEmptyPrompt 10000

        if ($geminiEmptyResponse -and $geminiEmptyResponse -match 'error') {
            Write-Pass "gemini empty prompt returned error as expected"
        } else {
            Write-Fail "gemini empty prompt did not return error: $geminiEmptyResponse"
        }
    } else {
        Write-TestHeader "gemini (NOT AVAILABLE)"
        Write-Skip "gemini tool not detected (Gemini CLI not installed?)"
    }
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 7: Gemini Image
# ═══════════════════════════════════════════════════════════════════

if ($SkipCli) {
    Write-TestHeader "gemini_image (SKIPPED)"
    Write-Skip "gemini_image skipped via -SkipCli flag"
} else {
    if ($toolsResponse -and $toolsResponse -match '"gemini_image"') {
        Write-TestHeader "gemini_image — 图像生成"

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $geminiImgResponse = Send-JsonRpc $proc $geminiImageRequest 120000
        $sw.Stop()

        if ($geminiImgResponse -and $geminiImgResponse -match '"result"') {
            Write-Pass "gemini_image returned result in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
            if ($geminiImgResponse -match 'SESSION_ID|session_id') {
                Write-Pass "  Response contains SESSION_ID"
            }
        } elseif ($null -eq $geminiImgResponse) {
            Write-Fail "gemini_image TIMEOUT (120s)"
        } else {
            Write-Fail "gemini_image error: $($geminiImgResponse.Substring(0, [Math]::Min(200, $geminiImgResponse.Length)))"
        }
    } else {
        Write-TestHeader "gemini_image (NOT AVAILABLE)"
        Write-Skip "gemini_image tool not detected (Gemini CLI not installed?)"
    }
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 8: Codex CLI
# ═══════════════════════════════════════════════════════════════════

if ($SkipCli) {
    Write-TestHeader "codex (SKIPPED)"
    Write-Skip "codex skipped via -SkipCli flag"
} else {
    if ($toolsResponse -and $toolsResponse -match '"codex"') {
        Write-TestHeader "codex — 简单问答"

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $codexResponse = Send-JsonRpc $proc $codexRequest 60000
        $sw.Stop()

        if ($codexResponse -and $codexResponse -match '"result"') {
            Write-Pass "codex returned result in $($sw.Elapsed.TotalSeconds.ToString('F1'))s"
            if ($codexResponse -match 'SESSION_ID|session_id|agent_messages') {
                Write-Pass "  Response contains expected fields"
            }
        } elseif ($null -eq $codexResponse) {
            Write-Fail "codex TIMEOUT (60s)"
        } else {
            Write-Fail "codex error: $($codexResponse.Substring(0, [Math]::Min(200, $codexResponse.Length)))"
        }

        Write-TestHeader "codex — 空 prompt 错误处理"

        $codexEmptyResponse = Send-JsonRpc $proc $codexEmptyPrompt 10000

        if ($codexEmptyResponse -and $codexEmptyResponse -match 'error') {
            Write-Pass "codex empty prompt returned error as expected"
        } else {
            Write-Fail "codex empty prompt did not return error: $codexEmptyResponse"
        }
    } else {
        Write-TestHeader "codex (NOT AVAILABLE)"
        Write-Skip "codex tool not detected (Codex CLI not installed?)"
    }
}

# ═══════════════════════════════════════════════════════════════════
#  TEST 9: 错误处理
# ═══════════════════════════════════════════════════════════════════

Write-TestHeader "Error handling — empty query"

$emptyQueryResponse = Send-JsonRpc $proc $emptyQueryRequest 5000

if ($emptyQueryResponse -and $emptyQueryResponse -match 'error|required') {
    Write-Pass "Empty query returned error as expected"
} else {
    Write-Fail "Empty query did not return error: $emptyQueryResponse"
}

Write-TestHeader "Error handling — empty URL"

$emptyUrlResponse = Send-JsonRpc $proc $emptyUrlRequest 5000

if ($emptyUrlResponse -and $emptyUrlResponse -match 'error|required') {
    Write-Pass "Empty URL returned error as expected"
} else {
    Write-Fail "Empty URL did not return error: $emptyUrlResponse"
}

# Cleanup session
$stderr = Stop-McpSession $proc

Write-Host "`n=== stderr (last 500 chars) ===" -ForegroundColor Yellow
if ($stderr.Length -gt 500) {
    Write-Host "...$($stderr.Substring($stderr.Length - 500))"
} else {
    Write-Host $stderr
}

# ═══════════════════════════════════════════════════════════════════
#  SUMMARY
# ═══════════════════════════════════════════════════════════════════

Write-Host "`n========================================" -ForegroundColor DarkGray
Write-Host "  RESULTS" -ForegroundColor White
Write-Host "========================================" -ForegroundColor DarkGray
Write-Host "  Passed:  $passed" -ForegroundColor Green
Write-Host "  Failed:  $failed" -ForegroundColor $(if ($failed -gt 0) {"Red"} else {"Green"})
Write-Host "  Skipped: $skipped" -ForegroundColor Yellow
Write-Host ""

if ($failed -gt 0) {
    Write-Host "  ❌ SOME TESTS FAILED" -ForegroundColor Red
    exit 1
} else {
    Write-Host "  ✅ ALL TESTS PASSED" -ForegroundColor Green
    exit 0
}
