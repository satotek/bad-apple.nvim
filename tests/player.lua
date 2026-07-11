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
bad_apple.stop()
print("player integration test passed")
