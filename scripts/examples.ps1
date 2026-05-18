@'
gh-usage release examples: copy any one line below into PowerShell.

cargo build --release -p gh-usage
.\target\release\gh-usage.exe
.\target\release\gh-usage.exe --help
.\target\release\gh-usage.exe --output .\target\gh-usage.csv
.\target\release\gh-usage.exe --format json --output .\target\gh-usage.json
.\target\release\gh-usage.exe --since-days 7 --output .\target\gh-usage-last-7-days.csv
.\target\release\gh-usage.exe --max-files 50 --output .\target\gh-usage-sample.csv
.\target\release\gh-usage.exe --no-bom --output .\target\gh-usage-no-bom.csv
Measure-Command { .\target\release\gh-usage.exe --output .\target\gh-usage.csv }
'@
