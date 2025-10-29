# TODO: remove this. Added to avoid running each command again and again for debugging.
# This is for building the triples module of thfhe.

rm -rf build
mkdir build && cd build
cmake ..
make
cp bin/test_triples ../test_triples