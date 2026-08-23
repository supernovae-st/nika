# nika serve operations

HTTP is an explicit door:

```sh
umask 077
printf 'your-32-plus-ascii-token\n' > .nika/serve.token
chmod 600 .nika/serve.token
nika serve --bind 127.0.0.1:8787 --workflows ./workflows --token-file .nika/serve.token
```

Bare `nika serve` stays the resident ARM firer.

- `nika-serve.service` — systemd unit, loopback, SIGTERM drain.
- `Caddyfile.nika-serve` — TLS + unbuffered SSE. Caddy terminates TLS;
  `nika doctor` never claims TLS from a proxy guess.
- Cancel and artifacts stay 404 until those authorities exist.
