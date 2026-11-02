<p align="center">
  <img src="https://raw.githubusercontent.com/Sheyiyuan/bonii/main/appicon.svg" width="180" alt="ボニー 吉祥物" />
</p>

<h1 align="center">ボニー &emsp; BONII</h1>

<p align="center"><strong>一次查询，并行查清一个名字在各处是否已被占用。</strong></p>

<p align="center">
  <img alt="CI" src="https://github.com/Sheyiyuan/bonii/actions/workflows/ci.yml/badge.svg" />
  <img alt="crates.io" src="https://img.shields.io/crates/v/bonii" />
  <img alt="Rust 2024" src="https://img.shields.io/badge/rust-2024-orange?logo=rust" />
  <img alt="License MIT" src="https://img.shields.io/badge/license-MIT-blue" />
</p>

**ボニー**（命令 `bonii`）把 GitHub、主流语言包管理仓库、Arch 系发行版仓库的命名空间查询收敛成一条命令。发布前最容易踩的坑之一就是名字冲突：`npm` 上没被占用不代表 PyPI 没被占用，AUR 上裸名空闲也不代表 `foo-git` 空闲。与其挨个打开十几个网站，不如使用 `bonii` 进行一次隐秘调查。

它会并行打完所有源，输出一张命中表，并给出可脚本化的退出码。

## 特性

- **一个名字，一圈源**：PyPI、npm、crates.io、RubyGems、NuGet、Homebrew、Arch 官方仓库、AUR、Packagist、Go module、GitHub。
- **真并行**：所有源同时查询，按目录顺序汇总，不看完成先后。
- **按生态归一化**：`Foo.bar` 与 `foo-bar` 在 PyPI 上是同一个名字（PEP 503）；crates.io 折叠 `-`/`_`；NuGet/Homebrew/Arch/AUR 转小写；npm 保留 scope 语义。
- **精确匹配，不是"搜到就算"**：GitHub 搜索返回的是相关度子串匹配，ボニー 在本地按 `owner/name` 精确过滤，避免把 `iusegit` 当成 `git`。
- **不给你假的"可用"**：输入形态不适用时显式跳过；查询失败时明确报错，而不是默认放行。
- **AUR 后缀探测**：额外检查 `-git`/`-bin`/`-nightly`/`-appimage`/`-desktop`，因为 AUR 惯例是把同一上游拆成多个包。
- **近似名扫描**：`--fuzzy` 会用 AUR suggest 找易混淆的名字，并逐个回查确认存在性。
- **双输出**：人类可读表格或 `--json`。

## 快速开始

```bash
cargo install bonii
bonii bonnie
```

示例（`--no-variants` 让 AUR 只显示裸名）：

```text
$ bonii bonnie --no-variants
PyPI           bonnie       可用
npm            bonnie       占用    Bonnie ============= __Alpha!__
crates.io      bonnie       占用    Simple, cross-platform, and fast command aliases with superpowers. — 2021-09-05
RubyGems       bonnie       可用
NuGet          bonnie       可用
Homebrew       bonnie       可用
Arch official  bonnie       可用
AUR            bonnie       占用    Simple, cross-platform, and fast command aliases with superpowers — orhun
Packagist      bonnie       跳过    该生态按带前缀的完整标识命名，裸名无法判定
Go module      bonnie       跳过    该生态按带前缀的完整标识命名，裸名无法判定
GitHub repos   bonnie       占用    至少 22 个同名仓库: arctic-hen7/bonnie(87★), MeasureAuthoringTool/bonnie(35★，已归档)…

占用 4 · 可用 5 · 跳过 2 · 错误 0
```

## 安装

### cargo（推荐）

```bash
cargo install bonii
```

二进制装到 `~/.cargo/bin/bonii`。需要 Rust 1.85+（项目使用 edition 2024）。想直接跟踪 main 分支可以改成从仓库安装：

```bash
cargo install --git https://github.com/Sheyiyuan/bonii --locked
```

### 预编译二进制（Linux / macOS）

```bash
curl -fsSLO https://github.com/Sheyiyuan/bonii/releases/latest/download/install.sh
sh install.sh
```

脚本会选对应平台的 release 产物、校验 SHA256，默认装到 `~/.local/bin`；如果该目录不在 `PATH` 里，会按登录 shell 写入对应配置（用 `--no-path` 关闭）。也可指定版本或前缀：

```bash
sh install.sh --version v0.1.0
sudo sh install.sh --prefix /usr/local
```

卸载用 `sh install.sh --uninstall`，会同时移除脚本写入的 PATH 配置。Windows 请直接下载 release 里的 `bonii_<version>_windows_amd64.zip`。

### 从源码

```bash
git clone https://github.com/Sheyiyuan/bonii && cd bonii
cargo build --release
./target/release/bonii bonnie
```

### 平台支持

| 平台    | 架构                     | 产物                 |
| ------- | ------------------------ | -------------------- |
| Linux   | x86_64 / aarch64         | musl 静态链接 tar.gz |
| macOS   | Intel / Apple Silicon    | tar.gz               |
| Windows | x86_64                   | zip                  |

### GitHub 查询

只依赖 Rust 工具链。GitHub 部分可选地复用已登录的 `gh` CLI（推荐，限额更高）：

```bash
gh auth login
```

没有 `gh` 时会降级到匿名 REST，能查但限额低，批量查询容易触发 403。

## 用法

```text
bonii <name>                    # 查询全部源
bonii @scope/name               # npm 作用域名（其余源显式跳过）
bonii github.com/owner/name     # Go 模块完整路径
bonii vendor/package            # Packagist 包名
bonii <name> --fuzzy            # 附加近似名扫描（当前实现 AUR）
bonii <name> -s aur -s crates   # 只查指定源
bonii <name> --exclude github   # 排除指定源
bonii <name> --no-variants      # 不探测 AUR 的 -git/-bin 等惯用后缀
bonii <name> --json             # 机器可读输出
bonii <name> --timeout 30       # 单请求超时（秒，默认 15）
bonii <name> --color always     # auto|always|never
```

`-s/--source` 与 `--exclude` 可重复；`--color` 默认 `auto`，遵循 `NO_COLOR`。

## 输出与退出码

退出码设计成可以直接放进脚本：

| 退出码 | 含义                                                     |
| ------ | -------------------------------------------------------- |
| `0`    | 全部源都给出结论，且没有任何命中（全绿）                 |
| `1`    | 至少一个源确认占用；即使另有源失败，也以"占用"为准       |
| `2`    | 用法或配置错误（未知源、无可查询源、catalog 解析失败等） |
| `3`    | 没有任何源确认占用，但至少一个源查询失败或结果不完整     |

最后一条很重要：**"查询失败"不等于"可用"**。例如 GitHub 搜索结果满页却没有精确同名仓库时，精确匹配可能被挤出了结果，ボニー 会报 `错误`（退出码 3），而不是错误地告诉你"可用"。

## 支持的源与判定契约

每个源在 [`src/catalog.toml`](src/catalog.toml) 里是一份声明式契约：查询 URL 模板、名字归一化规则、响应里取哪个字段、什么算命中。

| 源            | 端点                                            | 命中判定                |
| ------------- | ----------------------------------------------- | ----------------------- |
| PyPI          | `pypi.org/pypi/{n}/json`                        | 2xx                     |
| npm           | `registry.npmjs.org/{n}`                        | 2xx                     |
| crates.io     | `crates.io/api/v1/crates/{n}`                   | 2xx                     |
| RubyGems      | `rubygems.org/api/v1/gems/{n}.json`             | 2xx                     |
| NuGet         | `api.nuget.org/v3-flatcontainer/{n}/index.json` | 2xx                     |
| Homebrew      | `formulae.brew.sh/api/formula/{n}.json`         | 2xx                     |
| Arch official | `archlinux.org/packages/search/json/?name={n}`  | `results` 非空          |
| AUR           | `aur.archlinux.org/rpc/v5/info/{n}`             | `resultcount > 0`       |
| Packagist     | `packagist.org/packages/{vendor}/{n}.json`      | 2xx（需 `vendor/name`） |
| Go module     | `proxy.golang.org/{path}/@v/list`               | 2xx（需完整模块路径）   |
| GitHub        | `gh search repos` → 本地精确过滤                | 存在 `owner/{n}`        |

命中规则只有三种，足够覆盖所有现状：

- `status_ok`：只要 HTTP 2xx 就算命中，不解析响应体（纯文本端点）。
- `nonempty`：2xx 且 `extract` 指向的字段非空（数组/对象/字符串非空、数字非 0）。
- `greater = N`：2xx 且 `extract` 指向的数字大于 N。

新增一个常规源通常只需要在 [`src/catalog.toml`](src/catalog.toml) 里加一段 TOML，不用改代码。需要自定义协议的源（目前只有 GitHub）走专用模块。

## 四条实测踩出来的坑

改动源定义前请先读这几条，它们都是拿真实响应试出来的：

- **AUR 命中不看 HTTP 状态码。** 未占用的名字一样返回 200，只是 `resultcount: 0`。所以判定用 `resultcount > 0`，不能用 2xx。
- **crates.io 没有 User-Agent 一律 403。** 全局 UA 头是必需的，不是礼貌问题。
- **GitHub 搜索是"包含 + 相关度"，不是精确匹配。** 查 `git` 会返回 `iusegit`，所以必须本地按 `owner/name` 的 name 段做大小写不敏感精确过滤。另外 `--match name` 只覆盖公开仓库，且返回到上限时计数只能报成下界（输出里的"至少 N 个"）。
- **npm 的轻量端点已失效。** `/-/package/{n}/root` 现在对已存在和不存在的名都返回 404，只能取包文档。

## AUR 的两条特殊处理

1. **后缀探测**（默认开启）：AUR 惯例是把同一上游拆成 `foo-git`、`foo-bin`、`foo-appimage` 等包，所以裸名空闲不代表可用，会额外查这五个后缀。
2. **suggest 只可信其有，不可信其无。** `--fuzzy` 用 `/rpc/v5/suggest/{prefix}` 拿近似名，但实测 `suggest/bonnie` 能命中、`suggest/boni` 返回空，说明它是不完整的前缀索引（还会返回已删除的包）。因此每个候选都要回到 `info` 复核存在性，且返回空不能当成"没有近似名"的结论。整轮扫描有总预算（约 4×单请求超时），避免某个慢端点拖住整个查询。

## GitHub 集成

- **优先 `gh`，降级匿名 REST。** `gh auth status` 可用时走 `gh search repos`；否则走 `api.github.com`。
- **限额是独立计量的。** Search API 认证后 30 req/min，匿名 10 req/min（以响应头为准）。单次查询只花一个请求，差别在批量查询时才显现。
- **两条路径字段拼写不同**（`fullName` vs `full_name`），代码里统一处理，否则会静默把已占用判成空闲。
- **满页保护。** 搜索结果达到上限却没有精确同名仓库时，精确匹配可能被挤出结果，此时报"无法确认可用"（退出码 3），绝不返回假 `可用`。
- **超时一致。** `--timeout` 同时作用于 HTTP 源、`gh` 子进程和匿名 REST，`gh` 超时会被杀掉，不会无限挂起。

## 名字归一化

查询前按各生态规则归一化：

| 生态                          | 规则                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------ |
| PyPI                          | PEP 503：小写，`[-_.]+` 折叠为 `-`                                             |
| crates.io                     | 小写，`_` 折成 `-`                                                             |
| npm                           | 裸名小写；scoped 名字小写 scope、保留包名大小写，并对 `/` 之外字符做百分号编码 |
| NuGet / Homebrew / Arch / AUR | 小写                                                                           |
| GitHub                        | 小写后做精确比较                                                               |

输出表格的第二列是**归一化后的名字**（例如 scoped npm 包显示 `@scope/name`），实际请求 URL 会在其基础上再做百分号编码。

## 已知边界

- **Packagist 和 Go module 没有"裸名"概念**，只给完整标识时才查询（`vendor/package`、`github.com/owner/name`），否则显式跳过而不是报"可用"。
- **不查 npm scope 的占用**（scope 是另一层命名空间）。
- **不查域名、商标、社交账号。**
- **近似名扫描目前只做 AUR**：NuGet / Packagist / npm 没有可用的全量名清单端点。
- **GitHub 只覆盖公开仓库**；`--match name` 不包含私有仓库。
- 查询失败一律计入"错误"，不会被当成"可用"；退出码会把这一点暴露给脚本。

## 开发

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

模块划分：

```text
src/
  main.rs        CLI 参数、并发调度、输出与退出码
  catalog.rs     加载内置 catalog.toml
  catalog.toml   源声明（常规新源只需改这里）
  model.rs       serde 类型与命中规则
  query.rs       HTTP 查询、命中判定、近似扫描
  github.rs      GitHub 专用路径（gh / 匿名 REST / 本地精确过滤）
  normalize.rs   各生态名字归一化
  report.rs      表格渲染与着色
  util.rs        Hit 反序列化、截断、百分号编码
```

## 许可证

MIT，见 [LICENSE](https://github.com/Sheyiyuan/bonii/blob/main/LICENSE)。
