'use client';

import { useState } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import TaskTree from '@/components/TaskTree';
import DiffViewer from '@/components/DiffViewer';
import TmuxTerminal from '@/components/TmuxTerminal';
import { useTasks } from '@/hooks/useTasks';
import { Task, Agent } from '@/types';

export default function Home() {
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [diffContent, setDiffContent] = useState<string>('');
  const [terminalConnected, setTerminalConnected] = useState(false);
  
  const { tasks, isLoading, createTask, deleteTask } = useTasks();

  const selectedTask = tasks.find(task => task.id === selectedTaskId);
  const selectedAgent = selectedTask?.agents.find(agent => agent.id === selectedAgentId);

  const handleTaskSelect = (taskId: string) => {
    setSelectedTaskId(taskId);
    setSelectedAgentId(null); // Clear agent selection when selecting task
    setTerminalConnected(false);
    
    // TODO: Fetch combined diff for all agents in task
    setDiffContent('# Combined diff for all agents in task\n(Not yet implemented)');
  };

  const handleAgentSelect = async (taskId: string, agentId: string) => {
    setSelectedTaskId(taskId);
    setSelectedAgentId(agentId);
    setTerminalConnected(false);
    
    // Fetch agent-specific diff
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
      agents: undefined, // Use all agents
      name: undefined, // Generate random name
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