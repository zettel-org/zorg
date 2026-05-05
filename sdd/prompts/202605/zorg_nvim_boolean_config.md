---
plan: sdd/tales/202605/zorg_nvim_boolean_config.md
---
 The zorg-nvim plugin still seems to be causing the following error on on another machine. Can you help me diagnose the root cause of this issue and fix it? Think this through thoroughly and create a plan using your `/sase_plan` skill before making any file changes.

```
2026-05-05T12:50:48 lazy.nvim  ERROR Failed to run `config` for zorg-nvim

...bugyi/.local/share/nvim/lazy/zorg-nvim/lua/zorg/init.lua:4: attempt to index a boolean value

# stacktrace:
  - /zorg-nvim/lua/zorg/init.lua:4 _in_ **setup**
  - ~/.config/nvim/lua/plugins/zorg_nvim.lua:17 _in_ **config**
  - ~/.config/nvim/lua/config/lazy_plugins.lua:31
  - ~/.config/nvim/init.lua:50
```