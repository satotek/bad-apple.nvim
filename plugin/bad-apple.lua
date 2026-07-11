if vim.g.loaded_bad_apple_nvim then
  return
end
vim.g.loaded_bad_apple_nvim = true

vim.api.nvim_create_user_command("BadApplePlay", function(command)
  require("bad-apple").play(command.args ~= "" and command.args or nil)
end, {
  nargs = "?",
  complete = "file",
})

vim.api.nvim_create_user_command("BadAppleStop", function()
  require("bad-apple").stop()
end, {})

vim.api.nvim_create_user_command("BadApplePause", function()
  require("bad-apple").toggle_pause()
end, {})

vim.api.nvim_create_user_command("BadAppleInstall", function(command)
  require("bad-apple").install(command.bang)
end, { bang = true })
