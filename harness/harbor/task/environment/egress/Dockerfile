# Egress proxy: the only way out of the sandbox network. Allows CONNECT to
# hosts on the allowlist and nothing else. No TLS interception.
FROM alpine:3.20
RUN apk add --no-cache tinyproxy curl && mkdir -p /var/log/tinyproxy
COPY tinyproxy.conf /etc/tinyproxy/tinyproxy.conf
COPY allowlist /etc/tinyproxy/allowlist
EXPOSE 8888
HEALTHCHECK --interval=30s CMD curl -sf -x http://127.0.0.1:8888 -o /dev/null https://api.deepseek.com/ || exit 1
CMD ["tinyproxy", "-d", "-c", "/etc/tinyproxy/tinyproxy.conf"]
