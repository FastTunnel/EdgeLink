# EdgeLink Beacon

**EdgeLink Beacon** 是独立运行在 Linux 开发板上的 UDP 发现应答服务。监听 UDP `40191`，只响应从物理有线 Ethernet 网卡收到的 Windows 客户端（EdgeLink Router）定向广播查询，并从接收请求的有线网卡地址回复；Wi‑Fi 和虚拟网卡上的请求会被忽略。客户端从 UDP 响应源地址取得板子的当前 IP，因此板端不需要固定 IP。

## Debian 安装包构建与安装

在 Debian/Ubuntu 开发板或对应架构的 Linux 构建机上执行：

```bash
bash packaging/build-deb.sh
sudo apt install ../../dist/edgelink-beacon_*.deb
```

脚本默认按当前机器架构编译；也可以提供 Rust target 进行交叉构建，例如：

```bash
bash packaging/build-deb.sh --target aarch64-unknown-linux-gnu
```

交叉构建前需安装对应 Rust target 和链接器。包生成在仓库根目录的 `dist/` 下。安装后 systemd 会自动启用并启动 `edgelink-beacon.service`；服务以 systemd 动态分配的低权限账号（DynamicUser）运行。卸载包时会自动停止并禁用服务。

```bash
sudo systemctl status edgelink-beacon
sudo journalctl -u edgelink-beacon -f
```

如使用防火墙，请允许入站 UDP `40191`。

卸载：`sudo apt remove edgelink-beacon`。

发现协议定义在 `src/protocol.rs`，Windows 客户端（EdgeLink Router）包含对应实现；修改报文格式时请保持两边一致。
