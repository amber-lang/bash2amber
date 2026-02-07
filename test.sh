#!/bin/bash

# Pulse: A minimal system dashboard
# Colors
CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

draw_line() {
    printf "${CYAN}%.s─${NC}" $(seq 1 $(tput cols))
}

clear
while true; do
    # Get Data
    CPU_LOAD=$(top -bn1 | grep "Cpu(s)" | awk '{print $2 + $4}')
    MEM_USAGE=$(free -m | awk '/Mem:/ { printf("%3.1f%%", $3/$2*100) }')
    DISK_USAGE=$(df -h / | awk '/\// {print $5}')
    UPTIME=$(uptime -p | sed 's/up //')

    # Move cursor to top-left instead of clearing (prevents flickering)
    tput cup 0 0
    
    draw_line
    echo -e "${CYAN} SYSTEM PULSE ${NC} | Uptime: $UPTIME"
    draw_line

    # CPU Section
    echo -en "  CPU Usage:    "
    if (( $(echo "$CPU_LOAD > 80" | bc -l) )); then
        echo -e "${RED}[ $CPU_LOAD% ]${NC}  (High Load!)"
    else
        echo -e "${GREEN}[ $CPU_LOAD% ]${NC}"
    fi

    # Memory Section
    echo -e "  Memory:       ${GREEN}[ $MEM_USAGE ]${NC}"

    # Disk Section
    echo -e "  Disk (/):     ${GREEN}[ $DISK_USAGE ]${NC}"

    draw_line
    echo -e "  Press [CTRL+C] to exit..."
    
    sleep 2
done
