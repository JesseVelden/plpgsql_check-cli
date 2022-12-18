import os
import lldb

# Read the .env file and store the key-value pairs in a array with format ["key=value"]. 
# It only supports simple key-value pairs, no quotes or expansion.
# Comments are kinda working, because the key has a # in front of it.
env_array = []
with open(os.path.join(".env")) as f:
    for line in f:
        env_array.append(line.strip())


target = lldb.debugger.GetSelectedTarget()

launch_info = target.GetLaunchInfo()
launch_info.SetEnvironmentEntries(env_array, True)
target.SetLaunchInfo(launch_info)