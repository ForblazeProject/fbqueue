#!/bin/bash
# Focused test for Job ID filtering (qstat <ID>)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FBQ_BIN="$(realpath "$SCRIPT_DIR/../target/release/fbqueue")"
TEST_DIR="$SCRIPT_DIR/tmp_focused"

mkdir -p "$TEST_DIR"
export FBQUEUE_DIR="$TEST_DIR/.fbqueue"
rm -rf "$FBQUEUE_DIR"
mkdir -p "$FBQUEUE_DIR"

# Create symlinks
ln -sf "$FBQ_BIN" "$TEST_DIR/qsub"
ln -sf "$FBQ_BIN" "$TEST_DIR/qstat"
cd "$TEST_DIR"

echo "1. Submitting jobs..."
ID1_FULL=$(./qsub -N JobOne echo "1")
ID2_FULL=$(./qsub -N JobTwo echo "2")
ID1=$(echo $ID1_FULL | cut -d. -f1)
ID2=$(echo $ID2_FULL | cut -d. -f1)

echo "Captured ID1: $ID1_FULL -> $ID1"
echo "Captured ID2: $ID2_FULL -> $ID2"

sleep 2 # Wait for completion

echo -e "
2. Testing qstat <ID> (History search)..."
./qstat $ID1 > out1.txt
if grep -q "JobOne" out1.txt && ! grep -q "JobTwo" out1.txt; then
    echo "[PASS] qstat $ID1 worked."
else
    echo "[FAIL] qstat $ID1 failed."
    cat out1.txt
fi

echo -e "
3. Testing qstat <ID>.master (Suffix handle)..."
./qstat $ID2_FULL > out2.txt
if grep -q "JobTwo" out2.txt && ! grep -q "JobOne" out2.txt; then
    echo "[PASS] qstat $ID2_FULL worked."
else
    echo "[FAIL] qstat $ID2_FULL failed."
    cat out2.txt
fi

echo -e "
4. Testing mixed options: qstat -u $USER $ID1"
./qstat -u "$USER" $ID1 > out3.txt
if grep -q "JobOne" out3.txt && ! grep -q "JobTwo" out3.txt; then
    echo "[PASS] qstat -u \$USER $ID1 worked."
else
    echo "[FAIL] qstat -u \$USER $ID1 failed."
    cat out3.txt
fi
