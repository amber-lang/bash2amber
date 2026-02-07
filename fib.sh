#!/bin/bash

# 1. Ask the user for the number of terms
read -p "Enter the number of terms to generate: " N

# 2. Initialize the first two terms
a=0
b=1

echo "The Fibonacci sequence for $N terms is:"

# 3. Loop N times
for (( i=0; i<N; i++ ))
do
    echo -n "$a "
    
    # 4. Calculate the next term using Arithmetic Expansion
    fn=$((a + b))
    
    # 5. Update variables for the next iteration
    a=$b
    b=$fn
done

echo "" # Print a newline at the end
