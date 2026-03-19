#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

struct {
  __uint(type, BPF_MAP_TYPE_ARRAY);
  __uint(max_entries, 1);
  __type(key, __u32);
  __type(value, __u64);
} RUNNABLE_TASKS SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_ARRAY);
  __uint(max_entries, 1);
  __type(key, __u32);
  __type(value, __u64);
} BLOCKED_SYSCALLS SEC(".maps");

SEC("tracepoint/sched/sched_switch")
int handle_sched_switch(void *ctx) {
  __u32 key = 0;
  __u64 init = 0;
  __u64 *count = bpf_map_lookup_elem(&RUNNABLE_TASKS, &key);
  if (!count) {
    bpf_map_update_elem(&RUNNABLE_TASKS, &key, &init, BPF_ANY);
    count = bpf_map_lookup_elem(&RUNNABLE_TASKS, &key);
    if (!count) {
      return 0;
    }
  }
  __sync_fetch_and_add(count, 1);
  return 0;
}

SEC("tracepoint/raw_syscalls/sys_enter")
int handle_sys_enter(void *ctx) {
  __u32 key = 0;
  __u64 init = 0;
  __u64 *count = bpf_map_lookup_elem(&BLOCKED_SYSCALLS, &key);
  if (!count) {
    bpf_map_update_elem(&BLOCKED_SYSCALLS, &key, &init, BPF_ANY);
    count = bpf_map_lookup_elem(&BLOCKED_SYSCALLS, &key);
    if (!count) {
      return 0;
    }
  }
  __sync_fetch_and_add(count, 1);
  return 0;
}
