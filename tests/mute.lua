vim.opt.runtimepath:append(vim.fn.getcwd())

local bad_apple = require("bad-apple")
bad_apple.setup({
  engine_path = assert(vim.env.BAD_APPLE_TEST_ENGINE),
  movie_path = assert(vim.env.BAD_APPLE_TEST_MOVIE),
  audio_path = assert(vim.env.BAD_APPLE_TEST_AUDIO),
})
bad_apple.play()

local rendered = vim.wait(5000, function()
  return vim.fn.bufnr("bad-apple://player") >= 0
end, 20)
assert(rendered, "player did not start")

local mute_mapping = vim.fn.maparg("m", "n", false, true)
assert(type(mute_mapping.callback) == "function", "mute mapping was not installed")
mute_mapping.callback()
vim.wait(200)
mute_mapping.callback()
vim.wait(200)

assert(vim.fn.bufnr("bad-apple://player") >= 0, "player exited while toggling mute")
bad_apple.stop()
print("mute integration test passed")
