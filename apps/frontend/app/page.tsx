'use client';

import { useEffect, useState } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import TaskTree from '@/components/TaskTree';
import DiffViewer from '@/components/DiffViewer';
import TmuxTerminal from '@/components/TmuxTerminal';
import WorktreeList from '@/components/WorktreeList';
import WorktreeDetails from '@/components/WorktreeDetails';
import { useTasks } from '@/hooks/useTasks';
import { useWorktrees } from '@/hooks/useWorktrees';
import { WorktreeSummary } from '@/types';

const ENABLE_WORKTREE_DASHBOARD =
  process.env.NEXT_PUBLIC_ENABLE_WORKTREE_DASHBOARD === 'true';

export default function Home() {
  return ENABLE_WORKTREE_DASHBOARD ? <WorktreeDashboard /> : <TaskDashboard />;
}

function TaskDashboard() {
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [diffContent, setDiffContent] = useState<string>('');
  const [terminalConnected, setTerminalConnected] = useState(false);

  const { tasks, isLoading, createTask, deleteTask } = useTasks();

  const selectedTask = tasks.find((task) => task.id === selectedTaskId);
  const selectedAgent = selectedTask?.agents.find(
    (agent) => agent.id === selectedAgentId,
  );

  const handleTaskSelect = (taskId: string) => {
    setSelectedTaskId(taskId);
    setSelectedAgentId(null);
    setTerminalConnected(false);

    setDiffContent(
      '# Combined diff for all agents in task\n(Not yet implemented)',
    );
  };

  const handleAgentSelect = async (taskId: string, agentId: string) => {
    setSelectedTaskId(taskId);
    setSelectedAgentId(agentId);
    setTerminalConnected(false);

    try {
      const response = await fetch(`/api/tasks/${taskId}/agents/${agentId}/diff`);
      const diff = await response.text();
      setDiffContent(diff);
    } catch (error) {
      console.error('Failed to fetch diff:', error);
      setDiffContent('Error loading diff');
    }
  };

  const handleTerminalConnect = () => {
    if (selectedTaskId && selectedAgentId) {
      setTerminalConnected(true);
    }
  };

  const handleCreateTask = async (prompt: string) => {
    await createTask({
      prompt,
      agents: undefined,
      name: undefined,
    });
  };

  return (
    <MainLayout
      sidebar={
        <TaskTree
          tasks={tasks}
          isLoading={isLoading}
          selectedTaskId={selectedTaskId}
          selectedAgentId={selectedAgentId}
          onTaskSelect={handleTaskSelect}
          onAgentSelect={handleAgentSelect}
          onCreateTask={handleCreateTask}
          onDeleteTask={deleteTask}
        />
      }
      main={
        <DiffViewer
          diffText={diffContent}
          title={
            selectedAgent
              ? `${selectedAgent.name} (${selectedTask?.name})`
              : selectedTask
              ? `Combined diff - ${selectedTask.name}`
              : 'Select a task or agent to view changes'
          }
        />
      }
      bottom={
        <TmuxTerminal
          taskId={selectedTaskId}
          agentId={selectedAgentId}
          connected={terminalConnected}
          onConnect={handleTerminalConnect}
          disabled={!selectedTaskId || !selectedAgentId}
        />
      }
    />
  );
}

function WorktreeDashboard() {
  const { worktrees, isLoading } = useWorktrees();
  const [selectedWorktreeId, setSelectedWorktreeId] = useState<string | null>(
    null,
  );

  useEffect(() => {
    if (worktrees.length === 0) {
      setSelectedWorktreeId(null);
      return;
    }
    if (
      !selectedWorktreeId ||
      !worktrees.some((tree) => tree.id === selectedWorktreeId)
    ) {
      setSelectedWorktreeId(worktrees[0].id);
    }
  }, [worktrees, selectedWorktreeId]);

  const selectedWorktree: WorktreeSummary | null =
    worktrees.find((tree) => tree.id === selectedWorktreeId) ?? null;

  return (
    <MainLayout
      sidebar={
        <WorktreeList
          worktrees={worktrees}
          isLoading={isLoading}
          selectedId={selectedWorktreeId}
          onSelect={setSelectedWorktreeId}
        />
      }
      main={
        <WorktreeDetails worktree={selectedWorktree} isLoading={isLoading} />
      }
      bottom={
        <div className="h-full flex items-center justify-center text-sm text-gray-500">
          Command runner coming soon
        </div>
      }
    />
  );
}
