#!/bin/bash

# 自动获取第一个非 lo 的网卡
NETIF=$(ip -o link show | awk -F': ' '{print $2}' | grep -v lo | head -n 1)

if [ -z "$NETIF" ]; then
    echo " 找不到非 lo 网卡，退出"
    exit 1
fi

echo " 检测到目标网卡：$NETIF"

# 重置 tc 配置
reset_tc() {
    echo "🧹 清除 $NETIF 上的 tc 配置..."
    sudo tc qdisc del dev "$NETIF" root 2>/dev/null || true
    echo "已清除旧配置"
}

# 设置组合（延迟 + 带宽）
set_tc() {
    local delay=$1
    local rate=$2
    reset_tc
    echo "设置延迟 $delay 和带宽 $rate 到 $NETIF..."
    sudo tc qdisc add dev "$NETIF" root handle 1: htb default 10
    sudo tc class add dev "$NETIF" parent 1: classid 1:10 htb rate "$rate"
    sudo tc qdisc add dev "$NETIF" parent 1:10 handle 10: netem delay "$delay"
    echo " 设置完成：$NETIF ← delay=$delay, rate=$rate"
}

# 显示当前配置
show_tc() {
    echo " 当前 tc 配置："
    sudo tc qdisc show dev "$NETIF"
}

# 菜单
echo "请选择要应用的网络限制选项："
echo "1. 延迟 1ms, 带宽 1Gbit/s"
echo "2. 延迟 100ms, 带宽 100Mbit/s"
echo "3. 延迟 0.1ms, 带宽 10Gbit/s"
echo "4. 延迟 0.1ms, 带宽 1Gbit/s"
echo "5. 重置配置"
echo "6. 查看当前配置"
read -p "输入你的选择 (1-6): " choice

case "$choice" in
    1) set_tc "1ms" "1gbit" ;;
    2) set_tc "100ms" "100mbit" ;;
    3) set_tc "0.1ms" "10gbit" ;;
    4) set_tc "0.1ms" "1gbit" ;;
    5) reset_tc ;;
    6) show_tc ;;
    *) echo " 无效选项，退出。" ;;
esac
