# 壳语言的构建、检查与安装。
# 产物目录由 .cargo/config.toml 指定为「构建/」，release 可执行文件是「构建/release/壳」。
# 安装目标默认是 ~/bin/壳；二进制名沿用项目约定，仍是「壳」。

.PHONY: 全部 构建 测试 检查 安装 卸载 清理 帮助 install uninstall

# 默认装到当前用户的 ~/bin；可用 `make 安装 安装目录=/别的/目录` 覆盖。
安装目录 ?= $(HOME)/bin

# 直接敲 make 时：先出 release 产物，再做检查与测试。
全部: 构建 检查 测试

构建:
	cargo build --release --locked

# 测试走 dev profile（release 开了 panic=abort，测试框架无法捕获 panic）
测试:
	cargo test --locked

检查:
	cargo clippy --release -- -D warnings

安装: 构建
	@test -n "$(HOME)" || { echo "HOME 未设置，无法确定安装目录" >&2; exit 1; }
	install -d "$(安装目录)"
	install -m 755 "构建/release/壳" "$(安装目录)/壳"
	@echo "已安装：$(安装目录)/壳"

卸载:
	@test -n "$(HOME)" || { echo "HOME 未设置，无法确定安装目录" >&2; exit 1; }
	rm -f "$(安装目录)/壳"
	@echo "已卸载：$(安装目录)/壳"

清理:
	cargo clean

# 常见英文别名，方便习惯 `make install` 的调用方。
install: 安装
uninstall: 卸载

帮助:
	@echo "可用目标："
	@echo "  构建 / 测试 / 检查   编译、单测、clippy（零警告）"
	@echo "  安装                 构建后把「壳」装到 $(安装目录)/壳"
	@echo "  卸载                 删除 $(安装目录)/壳"
	@echo "  清理                 cargo clean"
	@echo "  install / uninstall  同 安装 / 卸载"
	@echo "直接 make 等价于 make 全部。"
