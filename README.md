# RPG Maker MZ 数据文件加解密工具

## 适用范围

所有 **RPG Maker MZ** 引擎（v1.9.x）部署的游戏。加密算法由引擎 `rmmz_managers.js` 的混淆代码实现，常量硬编码在引擎中，不随游戏变化。

RPG Maker MV 使用不同的加密方案，不适用此工具。

## 快速开始

### 方式一：一键还原（推荐）

解完就能直接运行和编辑，不需要反复加解密：

```bash
# 在游戏根目录执行
python3 rpgmz_crypt.py restore /path/to/game

# 之后直接编辑 data/ 中的文件，改完即生效

# 如需还原
python3 rpgmz_crypt.py revert /path/to/game
```

`restore` 做了三件事：
1. 备份加密数据到 `data.encrypted/`
2. 解密所有 `data/*.json` 为明文 JSON
3. 修改 `js/rmmz_managers.js`，让引擎同时支持加密和明文文件

### 方式二：传统加解密流程

如果不想修改 JS，只做数据转换：

```bash
# 1. 解密（保留原始格式）
python3 rpgmz_crypt.py decrypt ./data ./data_decrypted

# 2. 解密为可读格式（方便手动编辑）
python3 rpgmz_crypt.py decrypt ./data ./data_decrypted --pretty

# 3. 编辑后加密回去
python3 rpgmz_crypt.py encrypt ./data_decrypted ./data
```

## 命令行用法

```bash
# ── 文件加解密 ──
python3 rpgmz_crypt.py decrypt <加密目录> <输出目录> [--pretty]
python3 rpgmz_crypt.py encrypt <明文目录> <输出目录>
python3 rpgmz_crypt.py decrypt-file <输入.json> <输出.json> [--pretty]
python3 rpgmz_crypt.py encrypt-file <输入.json> <输出.json>

# ── 一键还原 / 撤销 ──
python3 rpgmz_crypt.py restore <游戏根目录>
python3 rpgmz_crypt.py revert <游戏根目录>

# ── 仅修改 JS 引擎 ──
python3 rpgmz_crypt.py patch-js <游戏根目录>
```

| 命令 | 说明 |
|------|------|
| `decrypt` | 批量解密目录中所有加密 JSON 文件 |
| `encrypt` | 批量加密目录中所有明文 JSON 文件 |
| `decrypt-file` | 解密单个文件 |
| `encrypt-file` | 加密单个文件 |
| `restore` | 一键还原：解密 data/ + 修改 JS，游戏可直接运行明文数据 |
| `revert` | 撤销 restore，恢复加密数据和原始 JS |
| `patch-js` | 仅修改 `rmmz_managers.js`，不碰数据文件 |

- `--pretty`：输出带缩进的格式化 JSON，方便阅读和编辑。不加则保留游戏原始格式
- 加密时无论输入是原始格式还是格式化 JSON，都能正确处理

## 工作流

### 一键还原（推荐）

```
原始游戏 ──restore──→ 可编辑游戏
  │                      │
  │  备份到               │  直接编辑 data/
  │  data.encrypted/      │  改完即生效
  │  rmmz_managers.js.bak │
  │                      │
  └──revert── 可以撤销 ──┘
```

### 传统加解密

```
加密数据 (data/)
    │
    └── decrypt ──→ 明文数据 (data_decrypted/)
                         │
                         ├── 修改、分析、翻译...
                         │
                         └── encrypt ──→ 加密数据 → 覆盖 data/
```

修改完数据后必须**加密回去**才能运行游戏。游戏引擎的 `DataManager.onXhrLoad` 会强制尝试解密所有加载的 JSON 文件——传明文给它会直接报错。

如果不希望反复加解密，使用 `restore` 命令一劳永逸。

---

# 加密原理

## 文件格式

游戏数据文件是一个包装了密文的 JSON：

```json
{
  "uid": "f1cdd4ab",
  "bid": "1.9.0",
  "data": "xBxeYXlihWd2AJ6UbHEm..."
}
```

| 字段 | 用途 |
|------|------|
| `uid` | 任意标识符，解密时不使用 |
| `bid` | 引擎版本号，也用作区分加密/明文的标记 |
| `data` | Base64 编码的密文 |

解密后 `data` 字段的内容是游戏的实际数据，如 `Actors.json` 解密后是角色数组，`Map001.json` 解密后是地图对象。

**注意**：地图 JSON 自身的 tile 数据也叫 `data`（数组），与加密包装的 `data`（base64 字符串）同名不同义。判断文件是否加密应该检查 `bid` 字段是否存在，而非 `data`。

## 密钥派生

密钥由**文件名**决定。引擎从加载路径中提取基础文件名，去掉 `.json` 后缀并转为小写，然后计算 JS 风格的字符串哈希：

```
filename_key = 247 XOR (string_hash(filename_stem.lower()) & 0xFF)
```

`247` 来自引擎中的 `Math.sqrt(61009) | 0`，且 `247² = 61009`。

字符串哈希算法是 Java/JavaScript 的标准实现：

```
hash = 0
for each char in string:
    hash = ((hash << 5) - hash + charCode) | 0   // 32-bit signed 截断
```

这意味着**不同文件的密文密钥不同**（`Actors.json` 和 `Map100.json` 的密钥不同），但**同一文件在不同游戏中密钥相同**（只要文件名一样）。

## 加密算法

核心是一个**带反馈的 XOR 流密码**，从数据末尾向开头逆序处理。

### 伪代码

```
ls = fk                    // 初始反馈值
for i = n-1 down to 0:     // 从最后一个字节逆序到第一个
    _c = fk XOR 82
    _m = i mod 128
    _p = (ls << 2) XOR (ls >>> 4)
    _k = (((_c + _m + _p) XOR 146) + 46) mod 256
    output[i] = input[i] XOR _k
    ls = plaintext[i]       // 反馈 = 当前位置的明文
```

### 关键设计点

| 特性 | 说明 |
|------|------|
| **逆序处理** | 从文件末尾向开头逐字节处理 |
| **明文反馈** | 下一个位置的密钥取决于当前明文字节，而非密文 |
| **位置感知** | 密钥中包含 `i mod 128`，相同明文字节在不同位置的密钥不同 |
| **文件名绑定** | 密钥派生依赖文件名哈希，换文件名解密会失败 |

### 为什么加密和解密是同一操作

加密和解密使用**相同的明文反馈**：

```
加密时：ls 追踪原始明文字节
解密时：ls 追踪恢复的明文字节（与原始相同）

因为 XOR 是对称的：c = p XOR k  →  p = c XOR k
且两边的反馈链相同（都追踪明文），所以密钥序列一致
```

这意味着加密函数和解密函数是镜像——结构相同，只是输入不同。

## 安全性评估

这个加密主要用于**防止普通用户直接读取游戏数据**，不是强密码学保护：

- **密钥可提取**：所有常量硬编码在 `rmmz_managers.js` 中
- **无随机性**：相同明文 + 相同文件名 = 相同密文（无 IV/salt）
- **文件名即为密钥**：20 个已知文件名的哈希空间有限
- **已知明文攻击**：引擎是公开的，算法完全可知

可以把它理解为一种**混淆/编码**而非真正的加密。

---

# 引擎代码位置

加解密逻辑在 `js/rmmz_managers.js` 第 107 行 `DataManager.onXhrLoad` 函数中。代码经过混淆，关键部分如下：

```javascript
// 设置常量
window._K = (Math.sqrt(61009)|0);    // = 247

// 解析响应，提取 data 字段
var c = JSON.parse(xhr.responseText);
var b = Buffer.from(c.data, 'base64');

// 从文件名派生密钥
var n = src.split(/[\\/]/).pop().replace('.json', '').toLowerCase();
var t = 0;
for (var i = 0; i < n.length; i++) t = ((t << 5) - t + n.charCodeAt(i)) | 0;

// 解密循环
var fk = (window._K | (t & 255)) & ~(window._K & (t & 255)), ls = fk;
for (var i = b.length - 1; i >= 0; i--) {
    var _c = (fk|82)&~(fk&82),                       // fk XOR 82
        _m = (i%128),                                 // i mod 128
        _p = ((ls<<2)|(ls>>>4))&~((ls<<2)&(ls>>>4)); // (ls<<2) XOR (ls>>>4)
    var _k = ((((_c+_m+_p)|146)&~(((_c+_m+_p)&146)))+46)&255;
    var v = (b[i]|_k)&~(b[i]&_k);                    // b[i] XOR _k
    b[i] = v;
    ls = v;                                           // 明文反馈
}

// 解析为游戏数据
window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));
```

混淆手法：`(a|b)&~(a&b)` 等价于 `a XOR b`，利用 JS 的 32-bit 位运算特性。

---

# Python 与 Rust 两个版本

提供两个等价的实现，功能完全相同，输出逐字节一致：

| 版本 | 入口 | 优点 |
|------|------|------|
| Python | `rpgmz_crypt.py` | 免编译，直接运行，方便修改 |
| Rust | `rpgmz_crypt_rs/` | 编译为单文件二进制，无运行时依赖，启动快 |

```bash
# Python
python3 rpgmz_crypt.py restore ./game

# Rust (Linux)
./rpgmz_crypt_rs/target/release/rpgmz_crypt restore ./game

# Rust (Windows)
rpgmz_crypt.exe restore ./game
```

## 从源码编译 Rust 版本

### Linux

```bash
# 1. 安装 Rust（如已安装跳过）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 编译
cd rpgmz_crypt_rs
cargo build --release
# 二进制位于 target/release/rpgmz_crypt (≈850KB, stripped)
```

### Windows

```powershell
# 1. 安装 Rust: https://rustup.rs
# 2. 把 rpgmz_crypt_rs/ 复制到 Windows
# 3. 在项目目录中编译
cargo build --release
# 输出: target\release\rpgmz_crypt.exe
```

### 从 Linux 交叉编译到 Windows

需要 MinGW 工具链：

```bash
# 安装 Windows 编译目标
rustup target add x86_64-pc-windows-gnu

# Arch Linux
sudo pacman -S mingw-w64-gcc

# Ubuntu/Debian
sudo apt install mingw-w64

# 编译
cargo build --release --target x86_64-pc-windows-gnu
# 输出: target/x86_64-pc-windows-gnu/release/rpgmz_crypt.exe
```

### 项目结构

```
rpgmz_crypt_rs/
├── Cargo.toml          # 依赖: clap, base64, serde_json, anyhow
├── src/
│   ├── main.rs         # 入口
│   ├── crypto.rs       # 加解密核心（JS 32-bit 模拟 + XOR 流密码）
│   ├── commands.rs     # 文件操作 / JS 补丁 / restore / revert
│   └── cli.rs          # 命令行定义（clap derive），7 个子命令
└── target/release/
    └── rpgmz_crypt     # 编译产物
```

已验证 Rust 与 Python 版本：
- 解密 **186 个数据文件** 输出逐字节一致
- 加密输出 base64 payload 逐字符一致
- `restore` / `revert` / `patch-js` 端到端正确

---

# restore 命令修改了什么

`restore` 命令对 `rmmz_managers.js` 做了两处精确替换，让引擎同时支持加密和明文文件：

```javascript
// 修改前（只支持加密文件）
var c = JSON.parse(xhr.responseText);
var b = Buffer.from(c.data, 'base64');
// ... 解密 ...
window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));
_t.onLoad(window[name]);

// 修改后（同时支持加密和明文）
var c = JSON.parse(xhr.responseText);
if (c.bid) {                              // ← 用 bid 而非 data 判断！
    var b = Buffer.from(c.data, 'base64'); // 加密文件 → 解密
    // ... 解密 ...
    window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));
} else {
    window[name] = c;                      // 明文文件 → 直接使用
}
_t.onLoad(window[name]);
```

**为什么用 `c.bid` 判断而非 `c.data`？** 因为地图 JSON 和事件 JSON 自身也有一个 `data` 属性（tile 数组和事件指令数组），用 `c.data` 会误判所有地图文件为加密文件，导致加载失败。只有加密包装 JSON 才有 `bid` 字段（值为 `"1.9.0"`），所以 `c.bid` 是最可靠的区分标记。

---

# 常见问题

**Q: 解密后文件名要保留原名吗？**
A: 是的。密钥依赖文件名。如果 `Map100.json` 改名为 `Map100_edit.json`，密钥会变化，解密和加密都不会正确。

**Q: 支持哪些数据文件？**
A: `data/` 目录下所有 `.json` 文件：Actors, Animations, Armors, Classes, CommonEvents, Enemies, Items, MapXXX, MapInfos, Skills, States, System, Tilesets, Troops, Weapons。

**Q: 游戏存档也加密吗？**
A: 存档使用不同的机制（`localforage` 存储），不受此工具影响。

**Q: RPG Maker MV 能用吗？**
A: 不能。MV 的加密方案不同（部分版本甚至不加密）。本工具仅适用于 MZ。

**Q: 其他引擎版本（1.8.x, 1.10.x）兼容吗？**
A: 核心算法和常量很可能是相同的（`sqrt(61009)` 和 82/146/46 是硬编码在混淆代码中的）。但如果引擎版本不同，可以检查 `rmmz_managers.js` 确认。

**Q: restore 之后游戏还能正常打开吗？**
A: 能。修改只影响数据文件加载逻辑，不破坏游戏功能。引擎自带的完整性校验只检查 game.exe，不检查 JS 或数据文件。

**Q: 如何撤销 restore？**
A: 运行 `python3 rpgmz_crypt.py revert <游戏目录>`。会从 `data.encrypted/` 和 `rmmz_managers.js.bak` 恢复原始文件。
