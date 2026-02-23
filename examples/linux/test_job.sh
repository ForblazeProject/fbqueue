#!/bin/bash
#$ -N LinuxExample
#$ -pe smp 2
#$ -o example_output.txt
echo "FBQueue Linux Example"
echo "Job ID: $FBQ_JOB_ID"
echo "Running on $(hostname)"
sleep 5
echo "Done"
