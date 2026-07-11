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

vim.api.nvim_create_user_command("BadApple", function()
  require("bad-apple").play()
end, {})
