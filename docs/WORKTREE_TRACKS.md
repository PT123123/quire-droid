# 并行 worktree 的实际坐标与踩坑（2026-09-22）

`docs/AGENT_HANDOFF.md` 第 8 行写了「四条同时开就必须各给一个 `git worktree` + 独立
`CARGO_TARGET_DIR`」，但没人落成文档，也没人真开过。现在开了一个，把**实测出来的**
 recipe、数字和三个反直觉的坑记在这里。四条 track 的 agent 都该读这一页。

本节所有数字都是本机（`C:` 953 G / **已用 96 %**，46 G 剩余）在 commit `78ddf35`
上量到的，不是估的。

---

## 1 · 为什么要开，以及它到底隔离了什么

一个 clone 四个 agent 同时改，共享的是**同一个工作树目录**：任何人 `git add`、任何
一次 `cargo fmt`、任何一次 `cargo build` 都会盖到别人正在编辑的文件上。worktree 让每
条 track 有**自己的目录和自己的 `target/`**，只共享 `.git` 里的对象库和 refs。

已经踩到的实例：主工作树在 `78ddf35` 之后同时挂着 T2/T3 的未提交改动（`PLAN.md`、
`docs/DECISIONS.md`、`docs/SPEC.md`、`src/core/mod.rs` + 新文件 `src/core/database.rs`）。
icon 切片收口时，`git add` 前必须先把别人的段落裁掉再还原。**在 worktree 里这类事情
不会发生**：`.scratch/wt/t1` 里 `grep -c "pub mod database" src/core/mod.rs` = 0，
`git status --short` 为空。

注意两点它**不**隔离：

- **refs 是共享的**。分支名全仓库唯一，且**一个分支不能被两个 worktree 同时 checkout**。
  主工作树此刻在 `track/3-database`，所以那条分支你不能再开。
- **`.gitignore` 是共享的，被忽略的东西不是**。`/.scratch`（`.gitignore:4`）里的基线截图、
  种子数据库、既有 worktree，都在**主仓库**的 `.scratch/` 下面，新 worktree 里根本没有。

## 2 · 开一个（照抄即可）

```bash
# 在主仓库根目录
git worktree add .scratch/wt/<tN> -b track/<N>-<slug> master
```

放在 `.scratch/` 下面是故意的：那一层被 gitignore，所以 `git status` 里看不见它，别的
agent 不会误 stage 它。目录名和分支名按你的 track 换。

用完删掉（**先确认自己的活已经提交或推走了**）：

```bash
git worktree remove .scratch/wt/<tN>
git worktree list          # 看还挂着谁
```

别手写删目录；`git worktree prune` 只在目录已经没了的时候清 ref。

## 3 · 坑一：**不要**把 `CARGO_TARGET_DIR` 指到 worktree 外面

`benchmarks/scripts/sweep.ps1` 的两条路径都是**相对当前工作目录**的，不是相对脚本：

- `sweep.ps1:23` — `$shot = "target\debug\quire-shot.exe"`
- `sweep.ps1:2` — `$OutDir = ".scratch/sweep"`

所以一旦 `export CARGO_TARGET_DIR=...` 指到外面，`cargo build --features software
--bin quire-shot` 建出来的东西就不在 `./target/debug/` 下，sweep 会打印
`build it first: cargo build ...` 然后 `exit 1`。看着像没编译，其实是路径断了。

**结论：worktree 里什么都不设，就用它自带的 `target/`。** 这本身就是「独立
`CARGO_TARGET_DIR`」——每个 worktree 天然有自己的 `target/`，不需要额外变量。
（同理，`justfile` 的 `shot` recipe 用 `.\target\debug\quire-shot.exe`，也一样要求默认位置。）

sweep 的输出目录建议显式给：`-OutDir .scratch/sweep-<slice>`。它落在 worktree 自己的
`.scratch/` 下，和主仓库的基线互不干扰。要拿旧基线做 diff，**-Baseline 必须给绝对路径**
（`<主仓库根>/.scratch/sweep33`），因为相对路径会从 worktree 根算起，那里没有 `sweep33`。
`<主仓库根>` 就是你 clone 的那个目录；本文不写死它的绝对路径，下同。

## 3b · 坑二：`-OutDir` 给**绝对**路径，sweep 一声不响地什么都不产出

同一条相对路径的病，长在另一头。`sweep.ps1:20` 是 `$ErrorActionPreference =
"SilentlyContinue"`，而保存那一行写的是：

```powershell
$img.Save((Join-Path (Get-Location) $png), ...)
```

`$png` 已经带上 `$OutDir` 了。所以 `-OutDir` 给**绝对**路径时，`Join-Path CWD
D:/abs/x.png` 拼出一个根本不存在的混合路径，`Save` 抛错，而 `SilentlyContinue`
把错误吃掉——**每张图都没落盘，退出码 0，一行输出都没有**。现场表现为
`manifest.txt` 里 72 行的 md5 全是空，跟基线一比就成了「变了 3 张、新增 5 张」这种
鬼话。这一次真的差点把 `menu.png` 的一个滚动条滑块读成一次弹窗尺寸变更。

**结论：`-OutDir` 一律给相对路径**（`.scratch/sweepNN`），绝对路径只在**读**基线时给
（`-Baseline` / `diffbbox.ps1 -OldDir`）。跑完第一件事不是看 diff，是数产物：

```bash
ls .scratch/sweepNN/*.png | wc -l                    # 要等于场景数，不是 0
grep -cE '^  [a-z]' .scratch/sweepNN/manifest.txt     # 空 md5 的行数，必须是 0
```

同理适用于任何 `SilentlyContinue` 的脚本：**静默不是阴性**。这条和
「先怀疑自己的验证脚本」是同一件事的两个面。

## 4 · 坑三：磁盘只有 46 G，而一个能跑像素闸的 worktree 就要 4.9 G

实测，全新的 worktree：

| 步骤 | 耗时 | 该 worktree 的 `target/` |
|------|------|--------------------------|
| `cargo check --all-targets`（冷） | 2 m 03 s | 1.4 G |
| `cargo build --features software --bin quire-shot` | 3 m 30 s | **4.9 G** |

两步都 `exit 0`、零警告。对比：主仓库的 `target/` 是 **43 G**（`debug` 37 G + `release`
5.9 G）。一个走完整 `just check`（check + test + `build --release`）的 worktree 大概
10–12 G，**四个全开就是 40–48 G，正好超过剩余空间**。所以：

- 只跑 `cargo check` 的 worktree 是便宜的（1.4 G），别在没收口前就 `build --release`。
- 一条 track 一个 worktree，**用完删**；`just clean` 只清当前目录的 `target`。
- 空间紧张时先 `du -sh .scratch/wt/*/target` 看谁最肥，再决定删哪个。

**收口时的实测，比上面那条估计更贵**：`t1` 这条 track 走完 `check --all-targets` +
`test --all-targets` + `build --release --all-targets` + `build --features software --bin
quire-shot` 之后，`du -sh target` = **13 G**（不是估的 10–12 G），本机 `C:` 剩余从写这一节
时的 46 G 掉到 **23 G（98 % 已用）**。含义很直接：**还能再开一个走完全程的 worktree，开不
起第二个**。所以四条全开的老建议在今天这台机器上不成立，实际可行的组合是「一条全量 +
两三条只 `cargo check`（1.4 G 那种）」。track 收口、推上去之后**立刻** `git worktree
remove`，那 13 G 是别人下一条 track 的立足之地。

## 5 · 像素基线是跨 worktree 可复现的（已验证，不是假设）

在 `.scratch/wt/t1` 里跑 `default` + `page-icon` 两场，和主仓库的基线逐字节相同：

```
7c4e872e264f2e5d43459f9693c19db9  .scratch/sweep33/default.png
7c4e872e264f2e5d43459f9693c19db9  .scratch/wt/t1/.scratch/sweep-smoke/default.png
e311ea18f180c2c7a2e09a62c938973e  .scratch/sweep33/page-icon.png
e311ea18f180c2c7a2e09a62c938973e  .scratch/wt/t1/.scratch/sweep-smoke/page-icon.png
```

`78ddf35` 这一版的 headless 软件渲染是确定性的。**含义**：你可以把主仓库的 `sweep33`
当 control，在 worktree 里跑 new，两边交替，RAM/像素闸的对照成立；但 `sweep33` 那份
基线本身不会被 clone 进 worktree，需要绝对路径引用。

**同名不是同物**。`.scratch/` 按 worktree 各自一份且都被 ignore，所以
`<主仓库根>/.scratch/sweep35` 和 `.scratch/wt/t1/.scratch/sweep35` 是两个毫无关系的目录
——今天主仓库那个 `sweep35` 里躺着 14 张别的 track 的半成品，而文档里说「baseline 是
sweep35」指的是 72 张那一份。**引用一个基线时要带上它在哪个 worktree**，或者干脆比较
md5 而不是比较名字。这也是为什么 `docs/UI_ARCHITECTURE.md` 里那句「baseline is
`.scratch/sweepNN`」只是一句约定，不是一个地址。

## 6 · 提交回 master 的规矩（共享工作树里仍然要）

worktree 里提交是干净的（只有一个 HEAD、只有你的改动）。**push 仍然要过 master**：

- 在自己的分支上 commit，然后 `git branch -f master <sha>` + `git push origin master:master`
  把 master 挪过去。**不要在主工作树 `git checkout master`**——那会碰别人正在改的文件，
  brief §2.1 明令禁止 checkout/switch/reset/stash/rebase。
- 快进不了（master 已被别的 track 推前）就 `git fetch` 后 rebase **在自己的 worktree 里**，
  不要 rebase 主工作树。
- 主工作树里那四个整合者文件（`PLAN.md` / `CHANGELOG.md` / `docs/ROADMAP.md` /
  `docs/DECISIONS.md`，brief §2.5）如果四个 track 都往里追加，提交前用「备份 → `sed -n
  '1,Np'` 裁到只含自己那段 → `git add` → 校验 `git diff --cached` 里没有别人的标记 →
  commit → 还原备份」这一套；还原后别人的未提交内容原样还在。
- **plumbing 挪了 master，就得把共享 index 一起对齐**（2026-09-23 实测的坑）。
  `update-ref` 只动 ref，`.git/index` 还停在旧 HEAD 上，于是这次提交里**新增**的那几个
  文件在 `git status` 里显示成 `D `（已暂存的删除），工作树里那一份反倒成了 `??`——
  下一个 track 一句 `git commit -a` 就把你的原始数据行从仓库里删掉。补法只碰自己的路径：
  `git update-index --cacheinfo 100644,$(git rev-parse HEAD:<path>),<path>`（新文件要加
  `--add`）。做完看两件事：`git status --porcelain` 里只剩别人原本的 ` M`，而
  `git diff HEAD -- <共享文件>` 全是他们自己的行、`-` 行为 0。
- ADR 号：四条 track 一律**追加在 `docs/DECISIONS.md` 末尾**（§2.4），只有整合者把收
  过来的 ADR 归位到文件头部。事实核对（2026-09-22）：这个文件是**降序**的（ADR-0001 在
  末尾），而 Track 1 的 0044–0050 六刀都直接落在头部第 5 行——`origin/master` 上的 0047
  就在那儿。两种做法都不出错，只要号不撞；新切片接着头部往下写最省事，也和文件现状一致。

## 7 · 现在的登记

| 位置 | 分支 | 归谁 | 状态 |
|------|------|------|------|
| 主工作树（仓库根本身） | `track/3-database`（HEAD） | 共享，四个 agent 都往里写 | 脏：挂着 T2/T3 的未提交改动 |
| `.scratch/wt/t1` | `track/1-page-appearance` | Track 1（版式 → icon → cover → lock → templates → version history 六刀在这里收口，§三十八 / M12 已交完） | version history 一刀在此收口：`target/` **13 G**，sweep 到 **83 张**（基线 `sweep39`，只存在于这个 worktree 的 `.scratch/` 里），像素闸与 `contrast_probe.ps1` 都在这条上跑通 |
| （无独立 worktree） | `track/2-references` | Track 2（引用 / 提及 / 反向链接） | `19201c3`，父 `d65ad3e`。**用 plumbing 在主工作树里造的**：`read-tree` + 逐文件 `update-index --cacheinfo` + `commit-tree` + `update-ref`，共享 HEAD 与工作树都没动。四个接缝文件（`core/mod.rs`、`storage/repository.rs`、`storage/migrations.rs`、`tests/integration/storage_test.rs`）按 hunk 摘掉了别 track 未提交的行；`docs/DECISIONS.md` / `docs/SPEC.md` 只进本 track 的 hunk。重建脚本 `.scratch/t2_build_commit.py`。T2.5 `synced block` 未做，ADR 号 0052 起 |
| （尚无） | — | Track 4 | 建议按 §2 各开一个，别在主工作树里建目录；**先读 §4 最后那段**，全量 worktree 开不起两个 |

master 与 `origin/master` 在 `78ddf35`（本节写下时的值；Track 1 的 cover 一刀随后把 master
往前挪，那一刀的 sha 以 `git log --oneline -1` 为准，不要以这里为准）。

**给 Track 3 的一条**：`pages.cover` 花掉了 schema **v12**（ADR-0047），同一天 `pages.locked`
花掉 **v13**（ADR-0048），还是同一天 `pages.template` 花掉 **v14**（ADR-0049），
`CURRENT_VERSION` 现在是 **14**。你那份草案里编号 14–17 的迁移步骤要整体往上挪一位起（database
那列如果是按 v14 写的，改成 v15）。别指望撞号会替你报错得很
清楚：`ensure_current`（migrations.rs:412，v13 之后；这行的号会随新步骤往下漂，认函数名别
认行号）
只在**进函数时**读一次 `user_version`，循环里的判断是 `migration.version <= from`，所以两
个都登记成 12 的步骤会**按注册顺序连着跑两遍**——撞在重复列名上就是一次
`migration <label> failed`，而被 `add_page_columns` 那种判存守卫挡下来的话就一句都不报，
最后写进 `user_version` 的是注册得靠后的那个。两种都不是你想要的信号。
