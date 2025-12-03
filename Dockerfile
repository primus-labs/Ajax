FROM fedora:latest
LABEL authors="hdvanegasm"

# Setup the system libraries
RUN <<EOF
dnf update -y --refresh
dnf install -y clang cmake openssl openssl-devel gmp gmp-devel gmp-c++ gmp-ecm-devel libmpc libmpc-devel python3 wget \
               curl
EOF

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup update

# Setup and install emp-tool
RUN wget https://raw.githubusercontent.com/emp-toolkit/emp-readme/master/scripts/install.py
RUN python install.py --deps --tool

# Setup library path
ENV LD_LIBRARY_PATH="/usr/local/lib"