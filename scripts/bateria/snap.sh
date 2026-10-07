#!/bin/bash
# Instantánea: tiempo, CPU (ticks) del proceso y red por uid (batterystats checkin, línea nt).
export PATH=$PATH:$HOME/Android/Sdk/platform-tools
D="-s ${ADB_DEV:-192.168.1.138:35325}"; P=org.lacre.app; UID_=10488
pid=$(adb $D shell pidof $P)
cpu=$(adb $D shell run-as $P cat /proc/$pid/stat | awk '{print $14+$15}')
nt=$(adb $D shell dumpsys batterystats --checkin | grep "^9,$UID_,l,nt," | head -1)
wake=$(adb $D shell dumpsys power | grep -m1 mWakefulness=)
echo "$(date +%s) pid=$pid cpu=$cpu $wake nt=$nt"
