vim.opt.runtimepath:append(vim.fn.getcwd())

local buffer = vim.api.nvim_get_current_buf()
local original = {
  "local function hello(name)",
  "  return 'hello, ' .. name",
  "end",
}
vim.api.nvim_buf_set_lines(buffer, 0, -1, false, original)
vim.keymap.set("n", "m", "<Cmd>let g:overlay_original_map = 1<CR>", { buffer = buffer })

local bad_apple = require("bad-apple")
bad_apple.setup({
  engine_path = assert(vim.env.BAD_APPLE_TEST_ENGINE),
  movie_path = assert(vim.env.BAD_APPLE_TEST_MOVIE),
  audio_path = vim.env.BAD_APPLE_TEST_AUDIO,
})
bad_apple.overlay()

local namespace = vim.api.nvim_create_namespace("bad-apple-overlay")
local rendered = vim.wait(5000, function()
  return #vim.api.nvim_buf_get_extmarks(buffer, namespace, 0, -1, {}) > 0
end, 20)
assert(rendered, "overlay did not create extmarks")
assert(vim.deep_equal(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), original), "overlay changed buffer text")
local mute_mapping = vim.fn.maparg("m", "n", false, true)
assert(type(mute_mapping.callback) == "function", "overlay mute mapping was not installed")
mute_mapping.callback()

bad_apple.overlay()
assert(#vim.api.nvim_buf_get_extmarks(buffer, namespace, 0, -1, {}) == 0, "overlay extmarks were not cleared")
assert(vim.fn.maparg("m", "n"):find("overlay_original_map", 1, true), "original mapping was not restored")
print("overlay integration test passed")
