vim.opt.runtimepath:append(vim.fn.getcwd())

local source = assert(vim.env.BAD_APPLE_TEST_MOVIE, "BAD_APPLE_TEST_MOVIE is required")
local bad_apple = require("bad-apple")

bad_apple.setup({ engine_path = vim.env.BAD_APPLE_TEST_ENGINE })
bad_apple.play(source)

local rendered = vim.wait(5000, function()
  local buffer = vim.fn.bufnr("bad-apple://player")
  return buffer >= 0 and vim.api.nvim_buf_line_count(buffer) > 1
end, 20)

assert(rendered, "player did not render a frame within five seconds")

local mappings = {}
for _, key in ipairs({ "h", "l", "r", "<Space>" }) do
  local mapping = vim.fn.maparg(key, "n", false, true)
  assert(type(mapping.callback) == "function", key .. " mapping was not installed")
  mappings[key] = mapping.callback
end

local buffer = vim.fn.bufnr("bad-apple://player")
local window = vim.fn.bufwinid(buffer)
local before_seek = table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), "\n")
mappings.l()
local sought = vim.wait(2000, function()
  return table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), "\n") ~= before_seek
end, 20)
assert(sought, "player did not render a different frame after seeking")

mappings.h()
mappings.r()
mappings["<Space>"]()
mappings["<Space>"]()

vim.api.nvim_win_set_width(window, math.max(vim.api.nvim_win_get_width(window) - 10, 20))
local expected_width = vim.api.nvim_win_get_width(window)
vim.api.nvim_exec_autocmds("WinResized", {})
local resized = vim.wait(2000, function()
  local lines = vim.api.nvim_buf_get_lines(buffer, 0, -1, false)
  return #lines > 1 and vim.fn.strchars(lines[1]) == expected_width
end, 20)
assert(resized, "player did not rerender at the resized window width")
assert(vim.api.nvim_buf_is_valid(buffer), "player exited while handling controls")

bad_apple.stop()
print("player integration test passed")
