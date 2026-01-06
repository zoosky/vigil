# My Network Monitor

Claude instructions:

/model
  ⎿  Set model to opus (claude-opus-4-5-20251101)

> make a PLAN.md: I am in a home network, with wlan or eth connection, going out to www via a zyxel fiber router. The connection drops more than once a day, for 1 to
 60 secs, then resumes. I need a tool that monitors the connection all day and logs the outages, and checks on the hops to find the culprit of the downtime. Use Rust
 to implement, call Macos tools via shell-out if needed and follwow best practices.

⏺ I'll create a comprehensive plan for your network monitoring tool.

> add a ./doc directory. Inside, document all features, archtitecture and useage guides if needed. in the ./docs/features write down the plan with 001-feature-x named
  files. add a claude.md for your own usage.