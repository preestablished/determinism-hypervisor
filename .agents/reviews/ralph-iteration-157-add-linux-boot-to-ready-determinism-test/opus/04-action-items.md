# 04-action-items.md

1. Add `ctx.log_fault()` checks to both detchannel PIO arms in `m9_service_exit_with_detchannel`.

2. Consider consuming cache entries, or rechecking artifact hashes after loading/opening, to remove the artifact mutation gap.

3. Consider focused unit coverage for `assert_no_host_input_before_ready`.
