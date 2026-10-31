# SPDX-License-Identifier: AGPL-3.0-or-later
# Developer-only dependency preparation. Installation below runs without a network.
FROM debian:trixie-20261027-slim
RUN apt-get update && apt-get install -y --no-install-recommends python3 openssl passwd systemd systemd-sysv libgcc-s1
WORKDIR /src
COPY tools/ tools/
COPY deploy/ deploy/
COPY payload/ payload/
RUN --network=none SYSTEMD_OFFLINE=1 python3 deploy/install_development.py --bundle payload --development --enable --name "Service fixture"
CMD ["/sbin/init"]
