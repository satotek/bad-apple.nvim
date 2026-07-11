local Player = {}
Player.__index = Player
local paths = require("bad-apple.paths")

local function read_u16(bytes, offset)
  local a, b = bytes:byte(offset, offset + 1)
  return a + b * 0x100
end

local function read_u32(bytes, offset)
  local a, b, c, d = bytes:byte(offset, offset + 3)
  return a + b * 0x100 + c * 0x10000 + d * 0x1000000
end

function Player.new(options)
  return setmetatable({
    options = options,
    buffer = nil,
    process = nil,
    stdin = nil,
    stdout = nil,
    stderr = nil,
    bytes = "",
    paused = false,
    muted = false,
    overlay = false,
    namespace = vim.api.nvim_create_namespace("bad-apple-overlay"),
    overlay_rows = {},
    saved_maps = {},
  }, Player)
end

function Player:install_controls(preserve)
  local controls = {
    q = function()
      require("bad-apple").stop()
    end,
    ["<Space>"] = function()
      require("bad-apple").toggle_pause()
    end,
    m = function()
      self:toggle_mute()
    end,
  }
  for lhs, callback in pairs(controls) do
    if preserve then
      local mapping = vim.fn.maparg(lhs, "n", false, true)
      if mapping and mapping.buffer and mapping.buffer ~= 0 then
        self.saved_maps[lhs] = mapping
      end
    end
    vim.keymap.set("n", lhs, callback, { buffer = self.buffer, silent = true })
  end
end

function Player:create_overlay()
  self.buffer = vim.api.nvim_get_current_buf()
  self.overlay = true
  self:install_controls(true)
  vim.api.nvim_create_autocmd("BufWipeout", {
    buffer = self.buffer,
    once = true,
    callback = function()
      self:stop(false)
    end,
  })
end

function Player:create_buffer()
  local buffer = vim.api.nvim_create_buf(false, true)
  self.buffer = buffer

  vim.bo[buffer].buftype = "nofile"
  vim.bo[buffer].bufhidden = "wipe"
  vim.bo[buffer].swapfile = false
  vim.bo[buffer].filetype = "bad-apple"
  vim.bo[buffer].modifiable = false
  vim.api.nvim_buf_set_name(buffer, "bad-apple://player")
  vim.api.nvim_win_set_buf(0, buffer)

  vim.wo.wrap = false
  vim.wo.number = false
  vim.wo.relativenumber = false
  vim.wo.signcolumn = "no"
  vim.wo.cursorline = false

  self:install_controls(false)

  vim.api.nvim_create_autocmd("BufWipeout", {
    buffer = buffer,
    once = true,
    callback = function()
      self:stop(false)
    end,
  })
end

function Player:apply_patch(payload)
  local message_type = payload:byte(1)
  if message_type ~= 1 and message_type ~= 2 then
    return
  end
  local row_count = read_u16(payload, 6)
  local cursor = 8
  local patches = {}
  for _ = 1, row_count do
    local row = read_u16(payload, cursor)
    local length = read_u32(payload, cursor + 2)
    cursor = cursor + 6
    patches[#patches + 1] = { row = row, text = payload:sub(cursor, cursor + length - 1) }
    cursor = cursor + length
  end

  vim.schedule(function()
    if not self.buffer or not vim.api.nvim_buf_is_valid(self.buffer) then
      return
    end
    if message_type == 2 then
      self:apply_overlay(patches)
    else
      vim.bo[self.buffer].modifiable = true
      for _, patch in ipairs(patches) do
        local line_count = vim.api.nvim_buf_line_count(self.buffer)
        if patch.row >= line_count then
          vim.api.nvim_buf_set_lines(
            self.buffer,
            line_count,
            -1,
            false,
            vim.fn["repeat"]({ "" }, patch.row - line_count + 1)
          )
        end
        vim.api.nvim_buf_set_lines(self.buffer, patch.row, patch.row + 1, false, { patch.text })
      end
      vim.bo[self.buffer].modifiable = false
    end
  end)
end

function Player:apply_overlay(patches)
  for _, patch in ipairs(patches) do
    self.overlay_rows[patch.row] = patch.text
  end
  vim.api.nvim_buf_clear_namespace(self.buffer, self.namespace, 0, -1)
  local line_count = vim.api.nvim_buf_line_count(self.buffer)
  for row, mask in pairs(self.overlay_rows) do
    if row < line_count then
      local line = vim.api.nvim_buf_get_lines(self.buffer, row, row + 1, false)[1] or ""
      local characters = vim.fn.strchars(line)
      local start = 1
      while start <= #mask and start <= characters do
        local state = mask:sub(start, start)
        local finish = start
        while finish < #mask and mask:sub(finish + 1, finish + 1) == state do
          finish = finish + 1
        end
        local start_byte = vim.fn.byteidx(line, start - 1)
        local end_byte = finish >= characters and #line or vim.fn.byteidx(line, finish)
        if start_byte >= 0 and end_byte > start_byte then
          vim.api.nvim_buf_set_extmark(self.buffer, self.namespace, row, start_byte, {
            end_col = end_byte,
            hl_group = state == "1" and "BadAppleOverlayLight" or "BadAppleOverlayDark",
            priority = 250,
          })
        end
        start = finish + 1
      end
    end
  end
end

function Player:consume(data)
  self.bytes = self.bytes .. data
  while #self.bytes >= 4 do
    local payload_size = read_u32(self.bytes, 1)
    if #self.bytes < payload_size + 4 then
      return
    end
    local payload = self.bytes:sub(5, payload_size + 4)
    self.bytes = self.bytes:sub(payload_size + 5)
    self:apply_patch(payload)
  end
end

function Player:start(source, overlay)
  local engine = paths.resolve_engine(self.options.engine_path)
  if not engine then
    error("bav-engine was not found; run cargo build or configure engine_path")
  end
  local movie = paths.resolve_movie(source or self.options.movie_path)
  if not movie then
    error("movie.bav was not found; run :BadAppleInstall or configure movie_path")
  end

  if overlay then
    self:create_overlay()
  else
    self:create_buffer()
  end
  local columns = math.max(vim.api.nvim_win_get_width(0), 20)
  local rows = math.max(vim.api.nvim_win_get_height(0) - 1, 8)
  local arguments = { movie, tostring(columns), tostring(rows) }
  local audio = paths.resolve_audio(self.options.audio_path)
  if audio then
    arguments[#arguments + 1] = audio
  end
  if overlay then
    arguments[#arguments + 1] = "--mask"
  end
  self.stdout = vim.uv.new_pipe(false)
  self.stderr = vim.uv.new_pipe(false)
  self.stdin = vim.uv.new_pipe(false)
  self.process = vim.uv.spawn(engine, {
    args = arguments,
    stdio = { self.stdin, self.stdout, self.stderr },
  }, function(code)
    if code ~= 0 then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: bav-engine exited with code " .. code, vim.log.levels.ERROR)
      end)
    end
  end)
  if not self.process then
    self.stdout:close()
    self.stderr:close()
    error("failed to start bav-engine")
  end

  self.stdout:read_start(function(error_message, data)
    if error_message then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: " .. error_message, vim.log.levels.ERROR)
      end)
    elseif data then
      self:consume(data)
    end
  end)
  self.stderr:read_start(function(_, data)
    if data and data ~= "" then
      vim.schedule(function()
        vim.notify("bad-apple.nvim: " .. vim.trim(data), vim.log.levels.ERROR)
      end)
    end
  end)
end

function Player:toggle_mute()
  if self.stdin and not self.stdin:is_closing() then
    self.muted = not self.muted
    self.stdin:write("m")
    vim.notify(self.muted and "bad-apple.nvim: Muted" or "bad-apple.nvim: Unmuted")
  end
end

function Player:toggle_pause()
  if not self.process then
    return
  end
  self.paused = not self.paused
  self.process:kill(self.paused and "sigstop" or "sigcont")
end

function Player:stop(close_buffer)
  for _, pipe in ipairs({ self.stdin, self.stdout, self.stderr }) do
    if pipe and not pipe:is_closing() then
      pipe:read_stop()
      pipe:close()
    end
  end
  self.stdout = nil
  self.stderr = nil
  self.stdin = nil

  if self.process and not self.process:is_closing() then
    self.process:kill("sigterm")
    self.process:close()
  end
  self.process = nil

  if self.overlay and self.buffer and vim.api.nvim_buf_is_valid(self.buffer) then
    vim.api.nvim_buf_clear_namespace(self.buffer, self.namespace, 0, -1)
    for _, lhs in ipairs({ "q", "<Space>", "m" }) do
      pcall(vim.keymap.del, "n", lhs, { buffer = self.buffer })
      if self.saved_maps[lhs] then
        vim.fn.mapset("n", false, self.saved_maps[lhs])
      end
    end
  end

  if not self.overlay and close_buffer ~= false and self.buffer and vim.api.nvim_buf_is_valid(self.buffer) then
    vim.api.nvim_buf_delete(self.buffer, { force = true })
  end
end

return Player
