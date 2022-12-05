defmodule ChatServer.Channels do
  @moduledoc """
  Keeps track of active channels. Transient.
  """
  use Agent

  def start_link(opts) do
    Agent.start_link(fn -> MapSet.new() end, opts)
  end

  def create(name) do
    Agent.get_and_update(__MODULE__, fn channels ->
      if MapSet.member?(channels, name) do
        {:error, channels}
      else
        {:ok, MapSet.put(channels, name)}
      end
    end)
  end

  def exists?(name) do
    Agent.get(__MODULE__, &MapSet.member?(&1, name))
  end

  def channels() do
    Agent.get(__MODULE__, & &1)
  end
end
