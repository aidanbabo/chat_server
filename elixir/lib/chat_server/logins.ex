defmodule ChatServer.Logins do
  @moduledoc """
  Manages login information. Transient.
  """
  use Agent

  def start_link(opts) do
    Agent.start_link(fn -> %{} end, opts)
  end

  def add(username, password) do
    Agent.get_and_update(__MODULE__, fn logins ->
      case logins do
        %{^username => _} -> {:error, logins}
        %{} -> {:ok, Map.put(logins, username, password)}
      end
    end)
  end

  def lookup(username, password) do
    Agent.get(__MODULE__, fn logins ->
      case logins do
        %{^username => ^password} -> :ok
        %{} -> :error
      end
    end)
  end
end
