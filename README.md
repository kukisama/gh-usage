# gh-usage 使用说明

`gh-usage` 用来查看本机 GitHub Copilot / Copilot Chat 的本地使用记录，重点统计：

- 一共消耗了多少信用点数
- 每天分别消耗了多少信用点数
- 平均每天大约消耗多少信用点数
- 扫描和统计花了多长时间
- 明细记录导出到 CSV 或 JSON，方便后续用 Excel、表格或脚本继续分析

它适合用来回答这些业务问题：

- 最近 Copilot 用量大不大？
- 哪几天消耗比较高？
- 总共大约消耗了多少 credits？
- 是否需要把明细导出来做进一步统计？

## 最常用：直接运行

如果你不想敲命令，也可以直接双击运行：

```text
target\release\gh-usage.exe
```

双击后程序会自动扫描并生成明细文件。因为它是命令行工具，窗口可能会一闪而过；这不影响结果生成。运行完成后，直接查看生成的 CSV 文件即可。

如果你想看到屏幕上的汇总信息，可以在项目根目录打开 PowerShell，复制这一行执行：

```powershell
.\target\release\gh-usage.exe
```

无论双击还是在 PowerShell 里运行，默认都会做两件事：

1. 在屏幕上显示汇总结果
2. 在当前目录生成一个明细文件：`copilot-usage.csv`

如果是双击运行，重点看生成的 `copilot-usage.csv`；如果是 PowerShell 运行，则可以同时看到屏幕汇总和 CSV 明细。

屏幕上大概会看到这样的内容：

```text
GitHub Copilot usage summary
output=copilot-usage.csv
records=101
total_credits=16679.800
active_days=2
avg_credits_per_active_day=8339.900

daily_credits:
  2026-05-17 records=25 credits=5558.600
  2026-05-18 records=76 credits=11121.200

scan_stats:
  scanned_files=774
  scanned_lines=73660
  candidate_lines=179
  parse_errors=0

timing_ms:
  discover_ms=11
  scan_ms=2074
  reduce_ms=0
  write_ms=0
  total_ms=2087
```

## 这些字段怎么看

### 总体结果

| 字段 | 含义 |
|---|---|
| `output` | 明细文件保存在哪里 |
| `records` | 找到了多少条可统计的 Copilot 消耗记录 |
| `total_credits` | 总信用点数消耗 |
| `active_days` | 有消耗记录的天数 |
| `avg_credits_per_active_day` | 平均每个有使用记录的日子消耗多少信用点数 |

### 每天消耗

`daily_credits` 是按日期汇总的结果：

```text
2026-05-18 records=76 credits=11121.200
```

意思是：

- `2026-05-18` 这一天
- 找到 `76` 条消耗记录
- 合计消耗 `11121.200` credits

### 扫描情况

| 字段 | 含义 |
|---|---|
| `scanned_files` | 扫描了多少个本地记录文件 |
| `scanned_lines` | 扫描了多少行本地记录 |
| `candidate_lines` | 其中多少行看起来可能包含 credits 信息 |
| `parse_errors` | 有多少行解析失败；正常情况下通常是 `0` |

### 执行耗时

`timing_ms` 里的单位都是毫秒：

| 字段 | 含义 |
|---|---|
| `discover_ms` | 找本地记录文件花了多久 |
| `scan_ms` | 扫描和解析记录花了多久 |
| `reduce_ms` | 去重、排序、汇总花了多久 |
| `write_ms` | 写出 CSV / JSON 文件花了多久 |
| `total_ms` | 整个命令总耗时 |

## 常用命令

### 查看帮助

```powershell
.\target\release\gh-usage.exe --help
```

### 指定 CSV 保存位置

```powershell
.\target\release\gh-usage.exe --output .\target\gh-usage.csv --summary
```

### 只看最近 7 天

```powershell
.\target\release\gh-usage.exe --since-days 7 --output .\target\gh-usage-last-7-days.csv --summary
```

### 导出 JSON

适合后面交给程序继续处理：

```powershell
.\target\release\gh-usage.exe --format json --output .\target\gh-usage.json --summary
```

### 简单测一下执行时间

```powershell
Measure-Command { .\target\release\gh-usage.exe --output .\target\gh-usage.csv --summary }
```

## 明细文件里有什么

默认会生成：

```text
copilot-usage.csv
```

这个 CSV 可以用 Excel 打开。它包含每条消耗记录的明细，例如：

- 使用时间
- Chat 标题
- 模型名称
- 本次消耗 credits
- 对应的本地记录文件
- 记录所在行号

CSV 默认带 Windows Excel 友好的编码标记，通常直接双击打开中文也不会乱码。

## 注意事项

- 这个工具统计的是本机能找到的本地记录，不等同于 GitHub 官方账单。
- 如果某些历史记录本身没有保存 credits 信息，工具就无法凭空还原。
- 统计结果适合做本地用量观察、趋势分析和粗略核对。
- 如果你只是想看结果，最推荐直接运行无参数命令。
