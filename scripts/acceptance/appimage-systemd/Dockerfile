FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive container=docker
RUN apt-get update && apt-get install -y --no-install-recommends systemd systemd-sysv dbus dbus-user-session libpam-systemd python3 xvfb xauth libgtk-3-0 libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libpulse0 libx11-xcb1 libxcb-randr0 libxcb-screensaver0 ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 tester && mkdir -p /var/lib/systemd/linger && touch /var/lib/systemd/linger/tester
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
