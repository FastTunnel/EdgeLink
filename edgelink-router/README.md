# EdgeLink Router (EdgeLink 路由助手)

EdgeLink Router 是 EdgeLink 棱镜互联体系的 Windows 桌面客户端与后台路由守护套件，面向嵌入式 Linux 开发板（如搭配 `edgelink-beacon` 的 RK3588/树莓派等设备），通过 UDP 局域网定向广播自动探测开发板在线状态并无感接管配置 Windows 本机双 `/1` 路由，免去繁琐的人工静态路由维护。

本工程构建三个独立的 Windows 二进制组件：

- **`edgelink-service.exe`**：Windows 后台守护服务（服务名：`EdgeLinkRouteService`）。独立于 UI 运行，按设定的扫描周期在目标网卡上探测开发板信标，并通过 Windows IP Helper 原生 API 维护两条 IPv4 `/1` 路由（下一跳指向开发板）。服务默认随 Windows 开机自动启动。若连续 3 次周期未探测到信标，将安全撤除托管路由并将网卡 IPv4 与 DNS 恢复为 DHCP 模式。
- **`edgelink-router.exe`**：基于 GPUI 打造的现代化深色极简配置客户端。带有 Windows 托盘常驻支持，关闭主窗口时退入系统托盘；托盘菜单支持一键启停后台服务或唤出配置中心。界面支持“路由与服务控制”及“网卡信息配置读取”双 Tab 视图，展示网卡详细指标（IP、MAC、MTU、速率、DHCP、DNS 及关键关联路由表）。
- **`edgelink-setup.exe`**：自解压安装包与卸载向导引擎，提供标准 Windows 安装程序、开始菜单快捷方式、桌面快捷方式与控制面板卸载集成（`uninstall.exe`）。

配置文件存储路径：`%PROGRAMDATA%\EdgeLink\settings.json`。

---

## 编译与打包安装程序

### 1. 编译二进制组件

在 Windows PowerShell 环境下运行：

```powershell
cargo build --release
```

编译产物位于 `target\release\`：
- `edgelink-router.exe`
- `edgelink-service.exe`
- `edgelink-setup.exe`

### 2. 生成单文件安装包

运行打包脚本，将生成独立的 Windows 安装向导可执行程序（位于 `../../dist/edgelink-router_0.1.0_x64_setup.exe`）：

```powershell
powershell -ExecutionPolicy Bypass -File packaging\build-installer.ps1 -Profile release
```

运行安装包 `edgelink-router_0.1.0_x64_setup.exe` 会触发 UAC 提权请求，并执行以下操作：
1. 部署主程序至 `C:\Program Files\EdgeLink\`（包含 `edgelink-router.exe`、`edgelink-service.exe`、`uninstall.exe` 及图标资产）。
2. 在 Windows 服务控制管理器 (SCM) 中自动注册并启动 `EdgeLinkRouteService` 服务（自启动模式）。
3. 在开始菜单与桌面创建“EdgeLink 路由助手”及“卸载 EdgeLink 路由助手”快捷方式。
4. 注册至 Windows 系统“已安装应用”与“添加/删除程序”，支持完整、彻底的向导化一键卸载。

---

## 运行与权限说明

- **免控制台黑框**：所有二进制编译时均注入 Windows 子系统属性（`windows_subsystem = "windows"`），双击运行时无黑色命令行弹窗。
- **最小化 UAC 权限请求**：GUI 界面以普通标准用户身份运行。仅在“保存共享配置”、“安装服务”、“启动/停止服务”或“卸载服务”时，才会通过内置提权器弹出一次 UAC 对话框确认。
- **托盘常驻与无缝退出**：点击窗口右上角关闭按钮时，程序隐入系统右下角托盘图标；右键托盘图标可快速启停服务或彻底退出客户端。
- **纯原生 API 调用**：网卡枚举与路由动态维护全程调用 Windows `Iphlpapi` (IP Helper)，网卡重置与 DHCP 恢复调用原生 COM/WMI 接口，全程零 PowerShell 外部进程消耗，安全可靠。

---

## 网络要求

- Windows 主机与运行 `edgelink-beacon` 的开发板须处于同一个二层广播域（局域网/交换机），且网络放行 UDP `40191` 定向广播。
- 协议内置随机 Nonce 防碰撞校验，保障广播交互的数据时效性。
