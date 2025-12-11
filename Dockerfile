FROM fedora:latest
LABEL authors="hdvanegasm"

# Setup the system libraries
RUN <<EOF
dnf update -y --refresh
dnf install -y clang cmake openssl openssl-devel gmp gmp-devel gmp-c++ gmp-ecm-devel libmpc libmpc-devel python3 wget \
               curl m4 nodejs npm git
EOF

# Setup and install emp-tool
RUN wget https://raw.githubusercontent.com/emp-toolkit/emp-readme/master/scripts/install.py
RUN python install.py --deps --tool --ot

# Setup library path
ENV LD_LIBRARY_PATH="/usr/local/lib"