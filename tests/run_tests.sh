#!/bin/bash

# FBQueue Automated Test Suite (Linux)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FBQ_BIN_REAL="$(realpath "$SCRIPT_DIR/../target/release/fbqueue")"
if [ ! -f "$FBQ_BIN_REAL" ]; then
    FBQ_BIN_REAL="$(realpath "$SCRIPT_DIR/../target/debug/fbqueue")"
fi

TEST_ROOT="$SCRIPT_DIR/tmp"
mkdir -p "$TEST_ROOT"
TEST_ROOT_ABS=$(cd "$TEST_ROOT" && pwd)

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

PASSED=0
FAILED=0
TEST_COUNT=0

stop_all_daemons() {
    PIDS=$(ps aux | grep "$FBQ_BIN_REAL daemon start" | grep -v grep | awk '{print $2}')
    if [ -n "$PIDS" ]; then
        kill -9 $PIDS > /dev/null 2>&1
        sleep 1
    fi
}

reset_state() {
    TEST_COUNT=$((TEST_COUNT + 1))
    stop_all_daemons
    export FBQUEUE_DIR="$TEST_ROOT_ABS/case_$TEST_COUNT"
    rm -rf "$FBQUEUE_DIR"
    mkdir -p "$FBQUEUE_DIR"
    WORK_DIR="$TEST_ROOT_ABS/work_$TEST_COUNT"
    rm -rf "$WORK_DIR"
    mkdir -p "$WORK_DIR"
    cd "$WORK_DIR"
    ln -sf "$FBQ_BIN_REAL" fbqueue
    ln -sf "$FBQ_BIN_REAL" qsub
    ln -sf "$FBQ_BIN_REAL" qstat
    ln -sf "$FBQ_BIN_REAL" qdel
    cat <<EOT > "$FBQUEUE_DIR/config"
capacity: 8
default_queue: batch
queue: batch
  priority: 10
EOT
    echo "Running Test Case $TEST_COUNT... (Dir: case_$TEST_COUNT)"
}

assert_exists() {
    if ls $1 >/dev/null 2>&1; then
        echo -e "  ${GREEN}[PASS]${NC} File(s) $1 exist."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} File(s) $1 do not exist."
        FAILED=$((FAILED + 1))
    fi
}

assert_grep() {
    if grep -qi "$2" "$1"; then
        echo -e "  ${GREEN}[PASS]${NC} Found '$2' in $1."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Could not find '$2' in $1."
        FAILED=$((FAILED + 1))
    fi
}

# --- Test Cases ---

test_basic_echo() {
    reset_state
    ./qsub echo "Hello FBQueue"
    sleep 3
    assert_grep "echo.o1" "Hello FBQueue"
}

test_script_no_x() {
    reset_state
    cat <<'EOT' > myscript.sh
#!/bin/bash
echo "Script working"
EOT
    chmod 644 myscript.sh
    ./qsub myscript.sh
    sleep 3
    assert_grep "myscript.sh.o1" "Script working"
}

test_pbs_directives() {
    reset_state
    cat <<'EOT' > pbs_test.sh
#!/bin/bash
#PBS -N PbsName
#PBS -l nodes=1:ppn=2
sleep 10
EOT
    ./qsub pbs_test.sh
    sleep 2
    ./qstat > stat.txt
    assert_grep "stat.txt" "PbsName"
    ./fbqueue stat --style default > stat_def.txt
    assert_grep "stat_def.txt" "COST: 2"
}

test_capacity_limit() {
    reset_state
    cat <<EOT > "$FBQUEUE_DIR/config"
capacity: 2
EOT
    ./qsub -N job1 sleep 10
    ./qsub -N job2 sleep 10
    ./qsub -N job3 sleep 10
    sleep 2
    ./qstat > stat.txt
    R_COUNT=$(grep -ic " R " stat.txt)
    Q_COUNT=$(grep -ic " Q " stat.txt)
    if [ "$R_COUNT" -eq 2 ] && [ "$Q_COUNT" -eq 1 ]; then
        echo -e "  ${GREEN}[PASS]${NC} Resource limit enforced (R:2, Q:1)."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Limit fail. R:$R_COUNT, Q:$Q_COUNT."
        FAILED=$((FAILED + 1))
    fi
}

test_priority_queue() {
    reset_state
    cat <<'EOT' > "$FBQUEUE_DIR/config"
capacity: 1
default_queue: batch
queue: batch
  priority: 10
queue: express
  priority: 100
EOT
    ./qsub -N blocker sleep 5
    ./qsub -q batch -N low_prio echo "low"
    ./qsub -q express -N high_prio echo "high"
    sleep 12
    assert_exists "high_prio.o3"
    assert_exists "low_prio.o2"
}

test_job_cancellation() {
    reset_state
    ./qsub -N kill_me sleep 20
    sleep 2
    ./qdel 1 > /dev/null
    sleep 2
    ./qstat > stat.txt
    if ! grep -qi "kill_me" stat.txt && [ -f "$FBQUEUE_DIR/queue/failed/1.job" ]; then
        echo -e "  ${GREEN}[PASS]${NC} Job cancellation verified."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job cancellation failed."
        FAILED=$((FAILED + 1))
    fi
}

test_daemon_recovery() {
    reset_state
    ./qsub -N interrupted_job sleep 20
    sleep 2
    stop_all_daemons
    if [ -f "$FBQUEUE_DIR/queue/running/1.job" ]; then
        echo -e "  ${GREEN}[PASS]${NC} Job state preserved."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job state lost."
        FAILED=$((FAILED + 1))
    fi
    ./qstat > /dev/null
    sleep 5
    ./qstat > stat_recovered.txt
    if grep -qi "interrupted_job" stat_recovered.txt && grep -qi " R " stat_recovered.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Job recovered and running."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Recovery failed. Stat content:"
        cat stat_recovered.txt
        FAILED=$((FAILED + 1))
    fi
}

test_delayed_start() {
    reset_state
    cat <<EOT > "$FBQUEUE_DIR/config"
capacity: 1
inactivity_timeout: 5
EOT
    echo "  Submitting job with 10s delay..."
    ./qsub -a +10s -N delayed_job echo "Delayed"
    sleep 2
    ./fbqueue stat > stat.txt
    if grep -qi "Wait until" stat.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Job is waiting with human-readable reason."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job not waiting correctly. Stat:"
        cat stat.txt
        FAILED=$((FAILED + 1))
    fi

    echo "  Waiting 15s to ensure daemon stays alive and job starts..."
    sleep 15
    if [ -f "delayed_job.o1" ]; then
        echo -e "  ${GREEN}[PASS]${NC} Job started after delay and completed."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job failed to start or daemon died. Stat:"
        ./qstat
        FAILED=$((FAILED + 1))
    fi

    echo "  Waiting 10s more to ensure daemon auto-shuts down after work..."
    sleep 10
    if ! ps aux | grep "$FBQ_BIN_REAL daemon start" | grep -v grep > /dev/null; then
        echo -e "  ${GREEN}[PASS]${NC} Daemon auto-shut down correctly."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Daemon still lingering after timeout."
        FAILED=$((FAILED + 1))
    fi
}

test_history_and_archiving() {
    reset_state
    cat <<EOT > "$FBQUEUE_DIR/config"
capacity: 10
history_limit: 3
archive_interval_days: 0
EOT
    echo "  Submitting 5 fast jobs..."
    for i in {1..5}; do
        ./qsub echo "Job $i"
    done
    sleep 5
    
    echo "  Checking history limit (should keep 3)..."
    ./fbqueue stat -H > history.txt
    COUNT=$(grep -c "ID:" history.txt)
    if [ "$COUNT" -eq 3 ]; then
        echo -e "  ${GREEN}[PASS]${NC} History limit enforced (kept $COUNT)."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} History limit failed (kept $COUNT). Stat:"
        cat history.txt
        FAILED=$((FAILED + 1))
    fi

    echo "  Waiting for background bundling (archive_interval_days=0)..."
    sleep 12
    ARCHIVE=$(ls "$FBQUEUE_DIR/archive/archive_"*.tar.gz 2>/dev/null)
    if [ -n "$ARCHIVE" ]; then
        echo -e "  ${GREEN}[PASS]${NC} Archive created: $(basename "$ARCHIVE")"
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Archive not created. Archive dir content:"
        ls -R "$FBQUEUE_DIR/archive"
        FAILED=$((FAILED + 1))
    fi
}

test_pbs_user_filter_and_history() {
    reset_state
    echo "  Submitting job with specific name..."
    ./qsub -N MyJob echo "test"
    sleep 1
    echo "  Checking qstat -u $USER (active or history)..."
    ./qstat -u "$USER" -H > stat_u.txt
    if grep -qi "MyJob" stat_u.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Found job with -u $USER."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Could not find job with -u $USER. Stat:"
        cat stat_u.txt
        FAILED=$((FAILED + 1))
    fi

    echo "  Checking qstat -u otheruser (should be empty)..."
    ./qstat -u "otheruser" -H > stat_other.txt
    if ! grep -qi "MyJob" stat_other.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Correctly filtered out other user."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Found job even with wrong user filter."
        FAILED=$((FAILED + 1))
    fi

    echo "  Checking qstat -H (PBS style history status)..."
    ./qstat -H > history_pbs.txt
    if grep -qi "MyJob" history_pbs.txt && grep -qi " F " history_pbs.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Found finished job in PBS history with 'F' status."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} PBS history failed. Stat:"
        cat history_pbs.txt
        FAILED=$((FAILED + 1))
    fi
}

test_pbs_job_id_filter() {
    reset_state
    echo "  Submitting job 1 and job 2..."
    ./qsub -N FirstJob echo "1"
    ./qsub -N SecondJob echo "2"
    sleep 1
    
    echo "  Checking qstat 1 (should only show FirstJob)..."
    ./qstat 1 > stat_1.txt
    if grep -qi "FirstJob" stat_1.txt && ! grep -qi "SecondJob" stat_1.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Correctly filtered by job ID 1."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job ID filtering failed. Stat:"
        cat stat_1.txt
        FAILED=$((FAILED + 1))
    fi

    echo "  Checking qstat 2.master (should handle suffix)..."
    ./qstat 2.master > stat_2.txt
    if grep -qi "SecondJob" stat_2.txt && ! grep -qi "FirstJob" stat_2.txt; then
        echo -e "  ${GREEN}[PASS]${NC} Correctly filtered by job ID 2.master."
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Job ID filtering with suffix failed. Stat:"
        cat stat_2.txt
        FAILED=$((FAILED + 1))
    fi
}

test_child_process_cleanup() {
    reset_state
    cat <<'EOT' > parent.sh
#!/bin/bash
sleep 100 &
child_pid=$!
echo "Child PID: $child_pid"
wait $child_pid
EOT
    chmod +x parent.sh
    ./qsub ./parent.sh
    sleep 2
    
    # Find the PID of the sleep command
    SLEEP_PID=$(pgrep -f "sleep 100")
    if [ -z "$SLEEP_PID" ]; then
        echo -e "  ${RED}[FAIL]${NC} Child process (sleep) not started."
        FAILED=$((FAILED + 1))
        return
    fi
    echo "  Detected child (sleep) PID: $SLEEP_PID"
    
    ./qdel 1
    sleep 3
    
    if ps -p $SLEEP_PID > /dev/null 2>&1; then
        echo -e "  ${RED}[FAIL]${NC} Child process $SLEEP_PID is still running! (Leak detected)"
        kill -9 $SLEEP_PID > /dev/null 2>&1
        FAILED=$((FAILED + 1))
    else
        echo -e "  ${GREEN}[PASS]${NC} Child process tree was correctly terminated."
        PASSED=$((PASSED + 1))
    fi
}

# --- Execution ---
test_basic_echo
test_script_no_x
test_pbs_directives
test_capacity_limit
test_priority_queue
test_job_cancellation
test_daemon_recovery
test_delayed_start
test_history_and_archiving
test_pbs_user_filter_and_history
test_child_process_cleanup
test_pbs_job_id_filter

echo "-----------------------------------"
echo -e "All Tests Finished: ${GREEN}$PASSED Passed${NC}, ${RED}$FAILED Failed${NC}"
stop_all_daemons

if [ $FAILED -ne 0 ]; then
    exit 1
fi
