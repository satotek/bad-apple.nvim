vim.opt.runtimepath:append(vim.fn.getcwd())

local bad_apple = require("bad-apple")
bad_apple.setup({ release_base = assert(vim.env.BAD_APPLE_TEST_RELEASE) })
bad_apple.play()

local rendered = vim.wait(5000, function()
  local buffer = vim.fn.bufnr("bad-apple://player")
  return buffer >= 0 and vim.api.nvim_buf_line_count(buffer) > 1
end, 20)

assert(rendered, "installed player did not render within five seconds")
bad_apple.stop()
print("installer integration test passed")
