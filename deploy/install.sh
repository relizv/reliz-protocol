#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  Reliz Protocol — One-Line Server Installer
#
#  Использование:
#    curl -sSL https://raw.githubusercontent.com/<repo>/main/deploy/install.sh | sudo bash
#
#  Или:
#    wget -qO- https://raw.githubusercontent.com/<repo>/main/deploy/install.sh | sudo bash
#
#  Что делает:
#    1. Ставит зависимости (build-essential, cmake, pkg-config, python3)
#    2. Ставит Rust (если нет)
#    3. Скачивает исходники
#    4. Компилирует ghost-server (release)
#    5. Устанавливает бинарник + CLI
#    6. Генерирует конфиг
#    7. Поднимает systemd-сервис
#    8. Создаёт первый клиентский токен
#    9. Выводит токен — готово
# ═══════════════════════════════════════════════════════════════════════════════

set -euo pipefail

# ── Цвета ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ── Пути ─────────────────────────────────────────────────────────────────────
INSTALL_DIR="/tmp/reliz-build"
GHOST_BIN="/usr/local/bin/ghost-server"
GHOST_CLI="/usr/local/bin/ghost"
GHOST_CONFIG_DIR="/etc/ghost"
GHOST_CONFIG="${GHOST_CONFIG_DIR}/ghost-server.conf"
REPO_URL="https://github.com/OWNER/reliz-protocol.git"  # ← ЗАМЕНИ НА СВОЙ РЕПО

# ── Утилиты вывода ──────────────────────────────────────────────────────────

banner() {
    echo ""
    echo -e "${MAGENTA}"
    echo "  ╔═══════════════════════════════════════════════════════╗"
    echo "  ║                                                       ║"
    echo "  ║    🚀  R E L I Z   P R O T O C O L                   ║"
    echo "  ║        One-Line Server Installer                      ║"
    echo "  ║                                                       ║"
    echo "  ╚═══════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

ok()   { echo -e "  ${GREEN}✔${NC} $1"; }
err()  { echo -e "  ${RED}✖${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚡${NC} $1"; }
info() { echo -e "  ${CYAN}ℹ${NC} $1"; }
step() { echo -e "\n  ${BOLD}${CYAN}── $1 ──${NC}\n"; }

# ── Проверки ────────────────────────────────────────────────────────────────

if [[ $EUID -ne 0 ]]; then
    err "Запусти от root: sudo bash install.sh"
    exit 1
fi

banner

# Определяем пакетный менеджер
if command -v apt-get &>/dev/null; then
    PKG_MGR="apt"
elif command -v dnf &>/dev/null; then
    PKG_MGR="dnf"
elif command -v yum &>/dev/null; then
    PKG_MGR="yum"
else
    err "Не найден пакетный менеджер (apt/dnf/yum)"
    exit 1
fi

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 1: Установка системных зависимостей
# ═══════════════════════════════════════════════════════════════════════════════

step "1/8 — Установка системных зависимостей"

case "$PKG_MGR" in
    apt)
        apt-get update -qq
        apt-get install -y -qq build-essential cmake pkg-config python3 git curl >/dev/null 2>&1
        ;;
    dnf|yum)
        $PKG_MGR install -y -q gcc gcc-c++ make cmake pkg-config python3 git curl >/dev/null 2>&1
        ;;
esac
ok "Зависимости установлены ($PKG_MGR)"

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 2: Установка Rust
# ═══════════════════════════════════════════════════════════════════════════════

step "2/8 — Rust toolchain"

# Проверяем Rust для текущего пользователя и для root
CARGO_BIN=""
if command -v cargo &>/dev/null; then
    CARGO_BIN="cargo"
    ok "Rust уже установлен: $(cargo --version)"
elif [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
    CARGO_BIN="cargo"
    ok "Rust найден: $(cargo --version)"
else
    info "Устанавливаю Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet
    source "$HOME/.cargo/env"
    CARGO_BIN="cargo"
    ok "Rust установлен: $(cargo --version)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 3: Скачивание исходников
# ═══════════════════════════════════════════════════════════════════════════════

step "3/8 — Скачивание исходников"

rm -rf "$INSTALL_DIR"

# Если скрипт запущен из директории с исходниками — используем их
if [[ -f "./Cargo.toml" ]] && grep -q "ghost-server" ./Cargo.toml 2>/dev/null; then
    INSTALL_DIR="$(pwd)"
    ok "Используем локальные исходники: $INSTALL_DIR"
else
    info "Клонирую репозиторий..."
    git clone --depth 1 "$REPO_URL" "$INSTALL_DIR" 2>/dev/null || {
        err "Не удалось клонировать $REPO_URL"
        err "Если репо приватный — скопируй исходники на сервер вручную и запусти:"
        err "  cd /path/to/reliz-protocol && sudo bash deploy/install.sh"
        exit 1
    }
    ok "Исходники скачаны"
fi

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 4: Компиляция
# ═══════════════════════════════════════════════════════════════════════════════

step "4/8 — Компиляция ghost-server (release)"

info "Это может занять 5-15 минут..."

# Создаём swap если мало памяти (< 2GB)
TOTAL_MEM=$(grep MemTotal /proc/meminfo | awk '{print $2}')
if [[ $TOTAL_MEM -lt 2000000 ]]; then
    if [[ ! -f /swapfile ]]; then
        warn "Мало RAM ($(( TOTAL_MEM / 1024 ))MB), создаю swap 2GB..."
        dd if=/dev/zero of=/swapfile bs=1M count=2048 status=none
        chmod 600 /swapfile
        mkswap /swapfile >/dev/null
        swapon /swapfile
        ok "Swap 2GB создан"
    fi
fi

cd "$INSTALL_DIR"
$CARGO_BIN build --release --bin ghost-server 2>&1 | tail -5
ok "Скомпилировано: target/release/ghost-server"

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 5: Установка бинарников
# ═══════════════════════════════════════════════════════════════════════════════

step "5/8 — Установка"

cp target/release/ghost-server "$GHOST_BIN"
chmod +x "$GHOST_BIN"
ok "ghost-server → $GHOST_BIN"

cp deploy/ghost.sh "$GHOST_CLI"
chmod +x "$GHOST_CLI"
ok "ghost CLI → $GHOST_CLI"

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 6: Генерация конфига
# ═══════════════════════════════════════════════════════════════════════════════

step "6/8 — Генерация конфигурации"

mkdir -p "$GHOST_CONFIG_DIR"

if [[ -f "$GHOST_CONFIG" ]]; then
    ok "Конфиг уже существует, пропускаю"
else
    AUTH_KEY=$(openssl rand -hex 32 2>/dev/null || python3 -c "import secrets; print(secrets.token_hex(32))")

    cat > "$GHOST_CONFIG" <<CONF
# Reliz Protocol Server Configuration
# Сгенерировано: $(date '+%Y-%m-%d %H:%M:%S')

listen_addr = "0.0.0.0:443"

allowed_users = [
]

enable_padding = true
max_padding_len = 64
enable_reality = true
mask_domain = "www.apple.com"
reality_auth_key = "${AUTH_KEY}"
verify_ja4 = false
allowed_ja4 = []
CONF

    ok "Конфиг создан: $GHOST_CONFIG"
fi

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 7: Systemd-сервис
# ═══════════════════════════════════════════════════════════════════════════════

step "7/8 — Systemd-сервис"

cat > /etc/systemd/system/ghost-server.service <<'SERVICE'
[Unit]
Description=Reliz Protocol Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/ghost-server /etc/ghost/ghost-server.conf
Restart=always
RestartSec=5
LimitNOFILE=65535
PrivateTmp=true
StandardOutput=journal
StandardError=journal
SyslogIdentifier=ghost-server

[Install]
WantedBy=multi-user.target
SERVICE

systemctl daemon-reload
systemctl enable ghost-server >/dev/null 2>&1
systemctl restart ghost-server
sleep 2

if systemctl is-active --quiet ghost-server; then
    ok "Сервис запущен и добавлен в автозагрузку"
else
    err "Сервис не запустился! Проверь: journalctl -u ghost-server -n 30"
    exit 1
fi

# Файрвол
if command -v ufw &>/dev/null; then
    ufw allow 443/tcp &>/dev/null && ok "UFW: порт 443 открыт"
elif command -v firewall-cmd &>/dev/null; then
    firewall-cmd --permanent --add-port=443/tcp &>/dev/null
    firewall-cmd --reload &>/dev/null && ok "Firewalld: порт 443 открыт"
fi

# ═══════════════════════════════════════════════════════════════════════════════
#  ШАГ 8: Генерация первого токена
# ═══════════════════════════════════════════════════════════════════════════════

step "8/8 — Генерация клиентского токена"

# Генерируем UUID
UUID=$(python3 -c "import uuid; print(uuid.uuid4().hex)" 2>/dev/null || \
       cat /proc/sys/kernel/random/uuid 2>/dev/null | tr -d '-' || \
       openssl rand -hex 16)

# Получаем IP сервера
SERVER_IP=$(curl -s --max-time 5 ifconfig.me 2>/dev/null || \
            curl -s --max-time 5 icanhazip.com 2>/dev/null || \
            hostname -I | awk '{print $1}')

MASK_DOMAIN=$(grep 'mask_domain' "$GHOST_CONFIG" | head -1 | sed 's/.*= *"//' | sed 's/".*//')
AUTH_KEY_VAL=$(grep 'reality_auth_key' "$GHOST_CONFIG" | head -1 | sed 's/.*= *"//' | sed 's/".*//')

# Добавляем UUID в конфиг
sed -i "s/^allowed_users = \[/allowed_users = [\n    \"${UUID}\",/" "$GHOST_CONFIG"
systemctl restart ghost-server
sleep 1

# Генерируем токен
TOKEN=$(python3 -c "
import json, base64
data = json.dumps({
    's': '${SERVER_IP}:443',
    'k': '${UUID}',
    'm': '${MASK_DOMAIN}',
    'a': '${AUTH_KEY_VAL}'
}, separators=(',', ':'))
b64 = base64.urlsafe_b64encode(data.encode()).decode().rstrip('=')
print('rlz_' + b64)
")

# ═══════════════════════════════════════════════════════════════════════════════
#  ГОТОВО
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${GREEN}"
echo "  ╔═══════════════════════════════════════════════════════╗"
echo "  ║                                                       ║"
echo "  ║   ✅  СЕРВЕР УСТАНОВЛЕН И ЗАПУЩЕН!                   ║"
echo "  ║                                                       ║"
echo "  ╚═══════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo ""
echo -e "  ${BOLD}Твой токен для подключения:${NC}"
echo ""
echo -e "  ${CYAN}${TOKEN}${NC}"
echo ""
echo -e "  ${DIM}Скопируй и вставь в клиент Reliz Protocol.${NC}"
echo -e "  ${DIM}Создать ещё токен: ghost key${NC}"
echo ""
echo -e "  ${BOLD}Управление:${NC}"
echo -e "    ghost status    — статус и логи"
echo -e "    ghost key       — новый токен"
echo -e "    ghost mask      — сменить маскировку"
echo -e "    ghost restart   — перезапустить"
echo ""
