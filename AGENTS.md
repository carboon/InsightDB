@SANDBOX.md
@RTK.md


# 🚨 Cross-Module Integration Rule (RTK + Sandbox)

When you are debugging deep issues (like long Java/Spring stack traces, complete dependency trees, or massive git diffs), the standard hooked commands might be overly compressed by RTK, causing you to lose context. 

If you suspect you are missing critical logs to solve the problem, you **MUST** use the raw proxy command defined in RTK.md:
- **Correct**: `rtk proxy mvn clean test`
- **Correct**: `rtk proxy git diff`
