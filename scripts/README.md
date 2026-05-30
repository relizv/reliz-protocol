# Сборка и интеграция tun2socks для Android

`tun2socks` — это userspace-демон, который превращает IP-пакеты из TUN-интерфейса в SOCKS5-соединения. Без него VPN-сервис создаст TUN, но трафик не попадёт в твой SOCKS5-прокси (127.0.0.1:10808).

## Быстрый старт (рекомендуется)

```powershell
# Вариант 1: попробовать скачать готовые библиотеки
.\scripts\fetch-prebuilt-tun2socks.ps1

# Вариант 2: собрать из исходников (требуется Linux/WSL + Android NDK)
bash scripts/build-tun2socks-android.sh
```

После выполнения в `ghost_flutter/android/app/src/main/jniLibs/` должны появиться:
```
arm64-v8a/libhev-socks5-tunnel.so
armeabi-v7a/libhev-socks5-tunnel.so
x86_64/libhev-socks5-tunnel.so
```

## Почему может быть UnsatisfiedLinkError

1. **Нет .so файлов** — не положили в `jniLibs/`
2. **Неправильная архитектура** — например, только `arm64-v8a`, а тестируете на эмуляторе x86_64
3. **Несовпадение JNI-имён** — `Tun2Socks.kt` ожидает:
   - `Java_com_example_reliz_protocol_Tun2Socks_tunnelStart`
   - `Java_com_example_reliz_protocol_Tun2Socks_tunnelStop`
   
   Если нативная библиотека экспортирует другие имена (например, `Java_hev_...`), нужен JNI-bridge.

## Альтернативы hev-socks5-tunnel

Если hev-socks5-tunnel не заводится:

1. **badvpn-tun2socks** — классика. Собирается через NDK, JNI имена другие.
2. **tun2socks из shadowsocks-android** — уже собран, ищи в их релизах.
3. **Запуск бинарника через Runtime.exec()** — вместо .so можно положить исполняемый файл `tun2socks` в `assets/` и запускать как отдельный процесс.

## Проверка перед релизом

```bash
# Убедись, что .so попали в APK
unzip -l app-release.apk | grep libhev-socks5-tunnel
```

## Где искать помощь

- hev-socks5-tunnel: https://github.com/heiher/hev-socks5-tunnel
- shadowsocks-android (примеры интеграции): https://github.com/shadowsocks/shadowsocks-android
