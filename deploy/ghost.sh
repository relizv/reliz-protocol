#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  Ghost Protocol — CLI Management Tool
#  Версия: 1.0.0
#
#  Установка:
#    sudo cp ghost.sh /usr/local/bin/ghost
#    sudo chmod +x /usr/local/bin/ghost
#
#  Использование:
#    ghost            → интерактивное меню
#    ghost setup      → установка и настройка сервиса
#    ghost status     → статус службы и логи
#    ghost key        → создать клиентский ключ
#    ghost mask       → сменить домен маскировки
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
NC='\033[0m' # No Color

# ── Пути ─────────────────────────────────────────────────────────────────────
GHOST_BIN="/usr/local/bin/ghost-server"
GHOST_CLI="/usr/local/bin/ghost"
GHOST_CONFIG_DIR="/etc/ghost"
GHOST_CONFIG="${GHOST_CONFIG_DIR}/ghost-server.conf"
GHOST_USERS="${GHOST_CONFIG_DIR}/allowed_users.txt"
GHOST_SERVICE="/etc/systemd/system/ghost-server.service"
GHOST_LOG_DIR="/var/log/ghost"

# ── Предустановленные домены для маскировки ─────────────────────────────────
MASK_DOMAINS=(
    "www.apple.com"
    "ads.x5.ru"
    "www.google.com"
    "www.microsoft.com"
    "cdn.cloudflare.com"
    "www.amazon.com"
    "play.google.com"
    "store.steampowered.com"
)

# ── Утилиты вывода ──────────────────────────────────────────────────────────
print_banner() {
    echo -e "${MAGENTA}"
    echo "  ╔═══════════════════════════════════════════════════════╗"
    echo "  ║                                                       ║"
    echo "  ║    👻  G H O S T   P R O T O C O L                   ║"
    echo "  ║        Stealth Proxy Management CLI                   ║"
    echo "  ║                                                       ║"
    echo "  ╚═══════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

log_ok() {
    echo -e "  ${GREEN}✔ $1${NC}"
}

log_err() {
    echo -e "  ${RED}✖ $1${NC}"
}

log_warn() {
    echo -e "  ${YELLOW}⚡ $1${NC}"
}

log_info() {
    echo -e "  ${CYAN}ℹ $1${NC}"
}

log_step() {
    echo -e "\n  ${BOLD}${CYAN}── $1 ──${NC}\n"
}

separator() {
    echo -e "  ${DIM}─────────────────────────────────────────────────${NC}"
}

# ── Проверки ────────────────────────────────────────────────────────────────

check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_err "Этот скрипт требует root-прав. Запустите: sudo ghost"
        exit 1
    fi
}

check_binary() {
    if [[ ! -x "$GHOST_BIN" ]]; then
        log_err "Бинарник $GHOST_BIN не найден или не исполняемый!"
        log_info "Сначала выполните: ghost setup"
        return 1
    fi
    return 0
}

check_config() {
    if [[ ! -f "$GHOST_CONFIG" ]]; then
        log_err "Конфиг $GHOST_CONFIG не найден!"
        log_info "Сначала выполните: ghost setup"
        return 1
    fi
    return 0
}

check_service() {
    if ! systemctl is-active --quiet ghost-server 2>/dev/null; then
        log_warn "Сервис ghost-server не запущен"
        return 1
    fi
    return 0
}

# ── Чтение/запись конфига ──────────────────────────────────────────────────

# Получить значение из TOML-конфига по ключу
config_get() {
    local key="$1"
    if [[ -f "$GHOST_CONFIG" ]]; then
        grep "^${key}=" "$GHOST_CONFIG" 2>/dev/null | head -1 | sed "s/^${key}=//" | tr -d '"' | tr -d ' '
    fi
}

# Установить значение в TOML-конфиг
config_set() {
    local key="$1"
    local value="$2"
    if [[ -f "$GHOST_CONFIG" ]]; then
        # Если ключ существует — заменяем
        if grep -q "^${key}=" "$GHOST_CONFIG"; then
            sed -i "s|^${key}=.*|${key}=\"${value}\"|" "$GHOST_CONFIG"
        else
            # Добавляем в конец файла
            echo "${key}=\"${value}\"" >> "$GHOST_CONFIG"
        fi
    fi
}

# Получить список allowed_users из конфига
get_allowed_users() {
    if [[ -f "$GHOST_CONFIG" ]]; then
        # Парсим TOML-массив: allowed_users = ["id1", "id2", ...]
        grep -A 100 '^allowed_users' "$GHOST_CONFIG" | grep -oP '"[0-9a-f]{32}"' | tr -d '"' | sort -u
    fi
}

# Добавить UUID в allowed_users
add_user_to_config() {
    local uuid="$1"
    if [[ -f "$GHOST_CONFIG" ]]; then
        # Проверяем, не существует ли уже
        if grep -q "\"${uuid}\"" "$GHOST_CONFIG"; then
            log_warn "UUID ${uuid} уже существует в конфиге"
            return
        fi

        # Получаем текущий массив пользователей
        local current_users
        current_users=$(grep -A 100 '^allowed_users' "$GHOST_CONFIG" | grep -oP '"[0-9a-f]{32}"' | tr -d '"')

        # Формируем новый массив
        local new_array="allowed_users = [\n"
        while IFS= read -r user; do
            if [[ -n "$user" ]]; then
                new_array+="    \"${user}\",\n"
            fi
        done <<< "$current_users"
        new_array+="    \"${uuid}\",\n"
        new_array+="]"

        # Удаляем старый блок allowed_users и вставляем новый
        # Используем Python для надёжной обработки TOML
        python3 -c "
import sys
content = open('$GHOST_CONFIG').read()
lines = content.split('\n')
new_lines = []
skip = False
for line in lines:
    if line.strip().startswith('allowed_users'):
        skip = True
        # Вставляем новый массив
        new_lines.append('allowed_users = [')
        users = '''$current_users'''.strip().split('\n') + ['$uuid']
        for u in users:
            u = u.strip()
            if u:
                new_lines.append('    \"' + u + '\",')
        new_lines.append(']')
        continue
    if skip:
        stripped = line.strip()
        if stripped == '' or stripped.startswith('#') or stripped.startswith('[') or (not stripped.startswith('"') and not stripped.startswith(',')):
            if stripped.startswith('[') or stripped.startswith('#'):
                skip = False
                new_lines.append(line)
            elif stripped == '':
                new_lines.append(line)
        else:
            continue
    else:
        new_lines.append(line)
open('$GHOST_CONFIG', 'w').write('\n'.join(new_lines))
" 2>/dev/null || {
            # Fallback: просто добавляем строку в конец секции allowed_users
            sed -i "/^allowed_users/,/^\]/ s/^\]/    \"${uuid}\",\n]/" "$GHOST_CONFIG" 2>/dev/null || \
            echo "    \"${uuid}\"," >> "$GHOST_CONFIG"
        }

        # Также пишем в файл пользователей
        echo "${uuid}" >> "$GHOST_USERS"
    fi
}

# ── Генерация конфига ──────────────────────────────────────────────────────

generate_auth_key() {
    openssl rand -hex 32 2>/dev/null || python3 -c "import secrets; print(secrets.token_hex(32))" 2>/dev/null || \
        echo "$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')"
}

generate_default_config() {
    local auth_key
    auth_key=$(generate_auth_key)

    mkdir -p "$GHOST_CONFIG_DIR"

    cat > "$GHOST_CONFIG" << CONF
# ═══════════════════════════════════════════════════════════════════════
#  Ghost Protocol Server Configuration
#  Путь: /etc/ghost/ghost-server.conf
#  Сгенерировано: $(date '+%Y-%m-%d %H:%M:%S')
# ═══════════════════════════════════════════════════════════════════════

# ── Сетевые настройки ─────────────────────────────────────────────────

# Адрес, на котором сервер принимает подключения
listen_addr = "0.0.0.0:443"

# ── Авторизация ───────────────────────────────────────────────────────

# Список разрешённых UUID (hex-строки, 32 символа)
# Генерируются через: ghost → Create Client Key
allowed_users = [
]

# ── Стелс: Dynamic Padding ────────────────────────────────────────────

# Включить рандомный паддинг в ответах сервера
enable_padding = true

# Максимальный размер паддинга (0–255 байт)
max_padding_len = 64

# ── Reality: Маскировка под легальный сервер ──────────────────────────

# Включить Reality-режим (TLS SNI masking)
enable_reality = true

# Домен, под который маскируется сервер (SNI + сертификат)
mask_domain = "www.apple.com"

# Приватный ключ авторизации Reality (64 hex символа = 32 байта)
reality_auth_key = "${auth_key}"

# ── JA4 Fingerprint Verification ─────────────────────────────────────

# Проверять JA4-отпечаток клиента
verify_ja4 = false

# Разрешённые JA4-отпечатки
allowed_ja4 = []
CONF

    log_ok "Конфиг создан: $GHOST_CONFIG"
    log_info "Reality Auth Key: ${auth_key}"
    echo ""
}

# ═══════════════════════════════════════════════════════════════════════════════
#  ОСНОВНЫЕ ФУНКЦИИ
# ═══════════════════════════════════════════════════════════════════════════════

# ── Setup: Установка и настройка ────────────────────────────────────────────

do_setup() {
    check_root
    print_banner
    log_step "SETUP: Установка Ghost Protocol Server"

    # 1. Проверяем бинарник
    if [[ ! -x "$GHOST_BIN" ]]; then
        log_err "Бинарник $GHOST_BIN не найден!"
        echo ""
        log_info "Скопируйте скомпилированный binary в $GHOST_BIN:"
        log_info "  sudo cp ghost-server $GHOST_BIN"
        log_info "  sudo chmod +x $GHOST_BIN"
        echo ""
        read -rp "  Указать путь к бинарнику? [y/N]: " choice
        if [[ "$choice" =~ ^[Yy]$ ]]; then
            read -rp "  Путь к ghost-server binary: " bin_path
            if [[ -f "$bin_path" ]]; then
                cp "$bin_path" "$GHOST_BIN"
                chmod +x "$GHOST_BIN"
                log_ok "Бинарник скопирован"
            else
                log_err "Файл $bin_path не найден"
                return 1
            fi
        else
            return 1
        fi
    else
        log_ok "Бинарник: $GHOST_BIN"
    fi

    # 2. Создаём конфиг
    if [[ ! -f "$GHOST_CONFIG" ]]; then
        log_step "Генерация конфигурации"
        generate_default_config
    else
        log_ok "Конфиг уже существует: $GHOST_CONFIG"
    fi

    # 3. Создаём директорию логов
    mkdir -p "$GHOST_LOG_DIR"
    log_ok "Директория логов: $GHOST_LOG_DIR"

    # 4. Создаём файл пользователей
    touch "$GHOST_USERS"
    log_ok "Файл пользователей: $GHOST_USERS"

    # 5. Устанавливаем systemd-сервис
    log_step "Настройка systemd-сервиса"

    cat > "$GHOST_SERVICE" << 'SERVICE'
[Unit]
Description=Ghost Protocol Server - Stealth Proxy
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/ghost-server /etc/ghost/ghost-server.conf
Restart=always
RestartSec=5
LimitNOFILE=65535

# Security
NoNewPrivileges=false
ProtectSystem=false
PrivateTmp=true

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=ghost-server

[Install]
WantedBy=multi-user.target
SERVICE

    log_ok "Systemd unit создан: $GHOST_SERVICE"

    # 6. Релоад и запуск
    systemctl daemon-reload
    log_ok "systemctl daemon-reload"

    systemctl enable ghost-server
    log_ok "Автозапуск при загрузке: включён"

    # 7. Открываем порт в файрволе
    if command -v ufw &>/dev/null; then
        ufw allow 443/tcp &>/dev/null && log_ok "UFW: порт 443/tcp открыт" || log_warn "UFW: не удалось открыть порт"
    elif command -v firewall-cmd &>/dev/null; then
        firewall-cmd --permanent --add-port=443/tcp &>/dev/null && \
        firewall-cmd --reload &>/dev/null && log_ok "Firewalld: порт 443/tcp открыт" || log_warn "Firewalld: не удалось открыть порт"
    else
        log_warn "Файрвол не обнаружен, убедитесь что порт 443 открыт"
    fi

    # 8. Запускаем сервис
    systemctl start ghost-server
    sleep 2

    if systemctl is-active --quiet ghost-server; then
        log_ok "Сервис ghost-server ЗАПУЩЕН"
    else
        log_err "Сервис ghost-server НЕ ЗАПУСТИЛСЯ"
        log_info "Проверьте логи: journalctl -u ghost-server -n 50"
        return 1
    fi

    separator
    echo ""
    log_ok "Установка завершена!"
    echo ""
    log_info "Текущий домен маскировки: $(config_get mask_domain)"
    log_info "Reality Auth Key: $(config_get reality_auth_key)"
    log_info "Порт: $(config_get listen_addr)"
    echo ""
    log_info "Следующие шаги:"
    log_info "  1. Создайте клиентский ключ: ghost key"
    log_info "  2. Передайте deep link клиенту"
    log_info "  3. Смените маскировку: ghost mask"
}

# ── Change Masking (Reality) ───────────────────────────────────────────────

do_mask() {
    check_root
    print_banner
    log_step "CHANGE MASKING: Смена домена Reality-маскировки"

    check_config || return 1

    local current_domain
    current_domain=$(config_get mask_domain)
    log_info "Текущий домен: ${current_domain}"
    echo ""

    echo -e "  ${BOLD}Выберите домен для маскировки:${NC}"
    echo ""
    local i=1
    for domain in "${MASK_DOMAINS[@]}"; do
        if [[ "$domain" == "$current_domain" ]]; then
            echo -e "  ${GREEN}${i}) ${domain} ${DIM}(текущий)${NC}"
        else
            echo -e "  ${i}) ${domain}"
        fi
        ((i++))
    done
    echo -e "  ${i}) Ввести свой домен"
    echo ""
    read -rp "  Ваш выбор [1-${i}]: " choice

    local new_domain=""

    if [[ "$choice" -ge 1 && "$choice" -le "${#MASK_DOMAINS[@]}" ]] 2>/dev/null; then
        new_domain="${MASK_DOMAINS[$((choice-1))]}"
    elif [[ "$choice" -eq "${i}" ]]; then
        read -rp "  Введите домен: " new_domain
        if [[ -z "$new_domain" ]]; then
            log_err "Пустой домен"
            return 1
        fi
    else
        log_err "Неверный выбор"
        return 1
    fi

    # Обновляем конфиг
    config_set mask_domain "$new_domain"
    log_ok "Домен маскировки изменён: ${new_domain}"

    # Перезапускаем сервис
    if check_service; then
        systemctl restart ghost-server
        sleep 2
        if systemctl is-active --quiet ghost-server; then
            log_ok "Сервис перезапущен"
        else
            log_err "Сервис не перезапустился!"
            log_info "Логи: journalctl -u ghost-server -n 30"
            return 1
        fi
    else
        log_warn "Сервис не запущен, перезапуск не нужен"
    fi

    echo ""
    log_ok "Маскировка обновлена: ${new_domain}"
    log_info "При сканировании цензор увидит легальный TLS-сервер ${new_domain}"
}

# ── Create Client Key ──────────────────────────────────────────────────────

do_key() {
    check_root
    print_banner
    log_step "CREATE CLIENT KEY: Генерация нового клиентского ключа"

    check_config || return 1

    # Генерируем UUID через uuidgen или python
    local uuid
    if command -v uuidgen &>/dev/null; then
        uuid=$(uuidgen | tr -d '-' | tr '[:upper:]' '[:lower:]')
    else
        uuid=$(python3 -c "import uuid; print(uuid.uuid4().hex)" 2>/dev/null)
    fi

    if [[ -z "$uuid" || ${#uuid} -ne 32 ]]; then
        log_err "Не удалось сгенерировать UUID"
        return 1
    fi

    # Получаем данные сервера
    local server_ip
    server_ip=$(curl -s --max-time 5 ifconfig.me 2>/dev/null || hostname -I 2>/dev/null | awk '{print $1}' || echo "YOUR_SERVER_IP")
    local server_port
    server_port=$(config_get listen_addr | sed 's/.*://' || echo "443")
    local mask_domain
    mask_domain=$(config_get mask_domain)
    local auth_key
    auth_key=$(config_get reality_auth_key)

    # Формируем base64-токен (rlz_...)
    local token
    token=$(python3 -c "
import json, base64
data = json.dumps({
    's': '${server_ip}:${server_port}',
    'k': '${uuid}',
    'm': '${mask_domain}',
    'a': '${auth_key}'
}, separators=(',', ':'))
b64 = base64.urlsafe_b64encode(data.encode()).decode().rstrip('=')
print('rlz_' + b64)
" 2>/dev/null)

    if [[ -z "$token" ]]; then
        log_err "Не удалось сгенерировать токен (нужен python3)"
        return 1
    fi

    # Добавляем UUID в конфиг
    add_user_to_config "$uuid"

    # Перезапускаем сервис, чтобы подхватил нового пользователя
    if check_service; then
        systemctl restart ghost-server
        sleep 1
    fi

    echo ""
    log_ok "Новый клиентский ключ создан!"
    separator
    echo ""
    echo -e "  ${BOLD}Token:${NC}"
    echo ""
    echo -e "  ${CYAN}${token}${NC}"
    echo ""
    echo -e "  ${DIM}Скопируйте токен и вставьте в клиент Reliz Protocol.${NC}"
    echo ""
    separator
    echo ""
    log_info "Всего пользователей: $(get_allowed_users | wc -l)"
}

# ── Status & Logs ──────────────────────────────────────────────────────────

do_status() {
    check_root
    print_banner
    log_step "STATUS & LOGS: Статус сервера"

    # Статус сервиса
    echo -e "  ${BOLD}Статус службы:${NC}"
    if systemctl is-active --quiet ghost-server 2>/dev/null; then
        echo -e "  ${GREEN}● ghost-server — ACTIVE${NC}"
    else
        echo -e "  ${RED}● ghost-server — INACTIVE${NC}"
    fi

    if systemctl is-enabled --quiet ghost-server 2>/dev/null; then
        echo -e "  Autostart: ${GREEN}enabled${NC}"
    else
        echo -e "  Autostart: ${RED}disabled${NC}"
    fi

    echo ""

    # Информация о конфиге
    if [[ -f "$GHOST_CONFIG" ]]; then
        echo -e "  ${BOLD}Конфигурация:${NC}"
        echo -e "    Listen:    $(config_get listen_addr)"
        echo -e "    Mask:      $(config_get mask_domain)"
        echo -e "    Reality:   $(config_get enable_reality)"
        echo -e "    Padding:   $(config_get enable_padding) (max $(config_get max_padding_len) bytes)"
        echo -e "    Users:     $(get_allowed_users | wc -l) UUID"
    fi

    echo ""

    # Проверка порта
    local port
    port=$(config_get listen_addr | sed 's/.*://' || echo "443")
    if ss -tlnp | grep -q ":${port} " 2>/dev/null; then
        echo -e "  Порт ${port}: ${GREEN}слушается${NC}"
    else
        echo -e "  Порт ${port}: ${RED}не слушается${NC}"
    fi

    echo ""

    # systemctl status
    echo -e "  ${BOLD}systemctl status:${NC}"
    systemctl status ghost-server --no-pager 2>/dev/null | head -15 || log_warn "Не удалось получить статус"
    echo ""

    # Последние 20 строк логов
    echo -e "  ${BOLD}Последние логи (journalctl):${NC}"
    separator
    journalctl -u ghost-server -n 20 --no-pager 2>/dev/null || log_warn "Не удалось получить логи"
}

# ── Uninstall ──────────────────────────────────────────────────────────────

do_uninstall() {
    check_root
    print_banner
    log_step "UNINSTALL: Удаление Ghost Protocol"

    echo -e "  ${RED}${BOLD}ВНИМАНИЕ! Это удалит все компоненты Ghost Protocol!${NC}"
    echo ""
    read -rp "  Вы уверены? [y/N]: " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        log_info "Отменено"
        return 0
    fi

    # Останавливаем сервис
    systemctl stop ghost-server 2>/dev/null || true
    systemctl disable ghost-server 2>/dev/null || true
    log_ok "Сервис остановлен и отключён"

    # Удаляем файлы
    rm -f "$GHOST_SERVICE"
    rm -f "$GHOST_BIN"
    rm -rf "$GHOST_CONFIG_DIR"
    rm -rf "$GHOST_LOG_DIR"
    rm -f "$GHOST_CLI"
    log_ok "Файлы удалены"

    systemctl daemon-reload
    log_ok "systemctl daemon-reload"

    echo ""
    log_ok "Ghost Protocol полностью удалён"
}

# ── Интерактивное меню ─────────────────────────────────────────────────────

show_menu() {
    check_root
    print_banner

    local current_mask
    current_mask=$(config_get mask_domain 2>/dev/null || echo "не настроен")
    local service_status
    if systemctl is-active --quiet ghost-server 2>/dev/null; then
        service_status="${GREEN}● RUNNING${NC}"
    else
        service_status="${RED}○ STOPPED${NC}"
    fi

    echo -e "  Сервер: ${service_status}    Маскировка: ${current_mask}"
    echo ""
    separator
    echo ""
    echo -e "  ${BOLD}1)${NC} Setup            — Установка и настройка сервера"
    echo -e "  ${BOLD}2)${NC} Status & Logs    — Статус службы и логи"
    echo -e "  ${BOLD}3)${NC} Create Client Key — Генерация UUID + Deep Link"
    echo -e "  ${BOLD}4)${NC} Change Masking   — Сменить домен Reality-маскировки"
    echo -e "  ${BOLD}5)${NC} Restart          — Перезапустить сервис"
    echo -e "  ${BOLD}6)${NC} Stop             — Остановить сервис"
    echo -e "  ${BOLD}7)${NC} Uninstall        — Полное удаление"
    echo -e "  ${BOLD}0)${NC} Exit"
    echo ""
    read -rp "  Выберите действие [0-7]: " action

    case "$action" in
        1) do_setup ;;
        2) do_status ;;
        3) do_key ;;
        4) do_mask ;;
        5)
            check_root
            systemctl restart ghost-server
            sleep 2
            if systemctl is-active --quiet ghost-server; then
                log_ok "Сервис перезапущен"
            else
                log_err "Ошибка перезапуска"
            fi
            ;;
        6)
            check_root
            systemctl stop ghost-server
            log_ok "Сервис остановлен"
            ;;
        7) do_uninstall ;;
        0) echo -e "\n  ${DIM}Пока! 👻${NC}\n"; exit 0 ;;
        *) log_err "Неверный выбор"; ;;
    esac
}

# ═══════════════════════════════════════════════════════════════════════════════
#  ТОЧКА ВХОДА
# ═══════════════════════════════════════════════════════════════════════════════

case "${1:-menu}" in
    setup|install|s)
        do_setup
        ;;
    status|stat|st)
        do_status
        ;;
    key|create-key|k)
        do_key
        ;;
    mask|change-mask|m)
        do_mask
        ;;
    restart|r)
        check_root
        systemctl restart ghost-server
        sleep 2
        if systemctl is-active --quiet ghost-server; then
            log_ok "Сервис перезапущен"
        else
            log_err "Ошибка перезапуска"
        fi
        ;;
    stop)
        check_root
        systemctl stop ghost-server
        log_ok "Сервис остановлен"
        ;;
    uninstall|remove)
        do_uninstall
        ;;
    menu|"")
        show_menu
        ;;
    help|--help|-h)
        print_banner
        echo "  Использование: ghost [команда]"
        echo ""
        echo "  Команды:"
        echo "    (пусто)     Интерактивное меню"
        echo "    setup       Установка и настройка сервера"
        echo "    status      Статус службы и логи"
        echo "    key         Создать клиентский ключ (UUID + Deep Link)"
        echo "    mask        Сменить домен Reality-маскировки"
        echo "    restart     Перезапустить сервис"
        echo "    stop        Остановить сервис"
        echo "    uninstall   Полное удаление"
        echo ""
        ;;
    *)
        log_err "Неизвестная команда: $1"
        log_info "Используйте: ghost help"
        exit 1
        ;;
esac
