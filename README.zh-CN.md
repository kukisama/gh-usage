# gh-usage

[English](README.md) | 简体中文

从 VS Code 和 Copilot CLI 本地记录快速生成 GitHub Copilot 使用量报告。

`gh-usage` 帮助个人和团队在不等待集中式报告的情况下了解本地 GitHub Copilot credit 使用情况。它会扫描机器上已经存在的使用记录，汇总结果，并同时写出适合电子表格分析的 CSV 数据和一个自包含的 HTML 报告。

它面向本地分析、内部复盘和运营可见性设计。它不能替代 GitHub 账单或官方使用量报告。

## 它能帮助回答什么问题

- 这台机器上找到了多少 Copilot credits？
- 哪些日期的使用量最高？
- 哪些模型和来源贡献了这些使用量？
- 哪些聊天或会话生成了详细记录？
- 多台机器之间的使用量如何对比？
- 是否可以在不搭建服务器的情况下，将结果作为简单报告分享？

## 亮点

- **本地优先：** 扫描当前机器上已经存在的文件。
- **速度快：** 使用 Rust 实现，适合处理较大的 VS Code 历史目录。
- **业务可读输出：** 在终端打印紧凑摘要，并写出便于 review 的 HTML 报告。
- **适合电子表格分析：** 默认导出 CSV，也可选择 JSON 用于自动化。
- **多机器聚合：** 将多台电脑生成的 CSV 合并为一个汇总报告。
- **跨平台：** 支持 Windows、Linux 和 macOS 的 VS Code 数据位置。

## 安装

### Windows

通过 Windows Package Manager 安装：

```powershell
winget install gh-usage
```

之后升级：

```powershell
winget upgrade gh-usage
```

### Linux 和 macOS

从 [Releases page](https://github.com/kukisama/gh-usage/releases) 下载匹配平台的压缩包，解压后运行 `gh-usage` 二进制文件。

## 快速开始

运行一次本地扫描：

```powershell
gh-usage
```

默认情况下，该命令会在当前目录写出两个文件：

- `copilot-usage-<machine>.csv`：用于电子表格分析的详细记录
- `copilot-usage-<machine>.html`：用于 review 和分享的交互式报告

终端也会打印一段紧凑摘要：

```text
+- GitHub Copilot Usage ---------------------------------+
| records                489  scanned files           82 |
| total credits      60122.2  candidate lines         49 |
| active days             14  parse errors             0 |
| avg / day           4294.4  total time          1.05 s |
+- Daily credits ----------------------------------------+
| 2026-06-02     74 records     10053.30 credits         |
| 2026-06-03     30 records      2942.00 credits         |
| 2026-06-04     42 records      2159.50 credits         |
+- Files ------------------------------------------------+
| csv   .\copilot-usage-workstation.csv                  |
| html  .\copilot-usage-workstation.html                 |
+--------------------------------------------------------+
```

上面的数字只是示例。你的报告取决于当前机器上可用的本地记录。

生成的 HTML 报告效果如下：

![gh-usage HTML 报告（中文）](design/image2.png)

## HTML 报告

HTML 报告是自包含文件，可以在任何浏览器中打开。它包含：

- 总记录数、总 credits、活跃天数，以及每个活跃日的平均 credits
- 每日使用量图表
- 按模型和来源拆分的统计
- 当存在合并数据时，展示按机器汇总
- 可搜索、可筛选的明细记录表
- 面向大型报告的分页
- 报告 UI 的语言切换

查看生成的报告不需要服务器、数据库或互联网连接。

## CSV 明细

CSV 每一行对应一条提取到的使用记录。常用字段包括：

- `hostname`：生成该记录的机器
- `local_time_hint`：可用时的本地时间戳
- `chat_title`：可用时的聊天标题
- `source`：记录来源，例如 VS Code chat history 或 Copilot CLI logs
- `model`：从记录中解析出的模型名称
- `credits`：该记录消耗的 credits
- `details`：原始 credit 明细文本
- `file`：被扫描的本地源文件
- `line`：源文件行号

CSV 默认包含 UTF-8 BOM，以改善 Windows Excel 兼容性。使用 `--no-bom` 可以关闭它。

## 常见使用场景

包含 GitHub Copilot CLI 日志：

```powershell
gh-usage --include-cli-logs
```

只扫描最近的记录：

```powershell
gh-usage --since-days 7
```

写入指定位置：

```powershell
gh-usage --output .\reports\copilot-usage.csv --html .\reports\copilot-usage.html
```

导出 JSON 而不是 CSV：

```powershell
gh-usage --format json --output .\reports\copilot-usage.json --no-html
```

跳过 HTML 报告：

```powershell
gh-usage --no-html
```

## 合并多台机器的报告

当涉及多台机器时，在每台机器上运行 `gh-usage`，然后将生成的 `copilot-usage-<machine>.csv` 文件收集到同一个文件夹中。

然后运行：

```powershell
gh-usage --merge .\shared\copilot-usage
```

合并模式会：

- 读取目标文件夹中的每一个 `copilot-usage-*.csv` 文件
- 完全跳过本地扫描
- 对重复记录进行去重
- 在 CSV 文件旁写出 `copilot-usage-merged.html`
- 在报告中保留按机器筛选和汇总

这适用于团队 review、设备迁移，或对比工作站和笔记本之间的使用情况。

## 数据范围和限制

- `gh-usage` 只扫描本地文件。
- 除非提供自定义路径，否则它会使用当前操作系统的标准 VS Code 用户数据位置。
- 已删除或不可用的本地历史无法重建。
- 没有 credit 明细的记录会被忽略。
- 结果面向分析和粗略对比，不适合作为官方记账依据。

## 常用选项

```text
--include-cli-logs       Include GitHub Copilot CLI records
--since-days <N>         Only scan files modified within the last N days
--output <PATH>          Write CSV or JSON to a specific path
--html <PATH>            Write the HTML report to a specific path
--no-html                Do not generate the HTML report
--merge [DIR]            Merge existing copilot-usage-*.csv files into one report
--format csv|json        Choose output format
--hostname <NAME>        Override the machine name stored in records
```

运行 `gh-usage --help` 查看完整命令参考。
