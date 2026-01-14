# TODO: remove this. Added to avoid running each command again and again for debugging.
# This is for building the triples module of thfhe.

rm -rf build
mkdir build && cd build || exit
cmake ..
make
cp bin/test_triples ../test_triples
LD_LIBRARY_PATH=$(pwd)/build:$LD_LIBRARY_PATH:/usr/local/lib
export LD_LIBRARY_PATH