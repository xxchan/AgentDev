'use client';

import { useState } from 'react';
import { Task, Agent, CreateTaskRequest } from '@/types';
import { 
  ChevronDownIcon, 
  ChevronRightIcon, 
  PlusIcon, 
  TrashIcon,
  PlayIcon,
  ExclamationTriangleIcon,
  CheckCircleIcon 
} from '@heroicons/react/24/outline';

interface TaskTreeProps {
  tasks: Task[];
  isLoading: boolean;
  selectedTaskId: string | null;
  selectedAgentId: string | null;
  onTaskSelect: (taskId: string) => void;
  onAgentSelect: (taskId: string, agentId: string) => void;
  onCreateTask: (prompt: string) => Promise<void>;
  onDeleteTask: (taskId: string) => Promise<void>;
}

export default function TaskTree({
  tasks,
  isLoading,
  selectedTaskId,
  selectedAgentId,
  onTaskSelect,
  onAgentSelect,
  onCreateTask,
  onDeleteTask,
}: TaskTreeProps) {
  const [expandedTasks, setExpandedTasks] = useState<Set<string>>(new Set());
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newTaskPrompt, setNewTaskPrompt] = useState('');
  const [creatingTask, setCreatingTask] = useState(false);

  const toggleTaskExpansion = (taskId: string) => {
    const newExpanded = new Set(expandedTasks);
    if (newExpanded.has(taskId)) {
      newExpanded.delete(taskId);
    } else {
      newExpanded.add(taskId);
    }
    setExpandedTasks(newExpanded);
  };

  const handleCreateTask = async () => {
    if (!newTaskPrompt.trim()) return;
    
    setCreatingTask(true);
    try {
      await onCreateTask(newTaskPrompt);
      setNewTaskPrompt('');
      setShowCreateModal(false);
    } catch (error) {
      console.error('Failed to create task:', error);
    } finally {
      setCreatingTask(false);
    }
  };

  const getAgentStatusIcon = (status: Agent['status']) => {
    if (typeof status === 'string') {
      switch (status) {
        case 'Ready':
          return <CheckCircleIcon className="w-4 h-4 text-green-500" />;
        case 'Running':
          return <PlayIcon className="w-4 h-4 text-blue-500" />;
        case 'Initializing':
          return <div className="w-4 h-4 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />;
        default:
          return <div className="w-4 h-4 bg-gray-400 rounded-full" />;
      }
    } else {
      // Error status
      return <ExclamationTriangleIcon className="w-4 h-4 text-red-500" />;
    }
  };

  const getAgentStatusText = (status: Agent['status']) => {
    if (typeof status === 'string') {
      return status;
    } else {
      return `Error: ${status.Error}`;
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b border-gray-200">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-gray-900">Tasks</h2>
          <button
            onClick={() => setShowCreateModal(true)}
            className="flex items-center px-3 py-1.5 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors"
          >
            <PlusIcon className="w-4 h-4 mr-1" />
            New Task
          </button>
        </div>
      </div>

      {/* Task list */}
      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="p-4 text-center text-gray-500">
            <div className="inline-block w-6 h-6 border-2 border-gray-300 border-t-blue-500 rounded-full animate-spin mb-2"></div>
            <p>Loading tasks...</p>
          </div>
        ) : tasks.length === 0 ? (
          <div className="p-4 text-center text-gray-500">
            <p>No tasks yet</p>
            <p className="text-sm mt-1">Create your first task to get started</p>
          </div>
        ) : (
          <div className="py-2">
            {tasks.map((task) => (
              <div key={task.id} className="mb-1">
                {/* Task row */}
                <div
                  className={`flex items-center px-4 py-2 hover:bg-gray-50 cursor-pointer ${
                    selectedTaskId === task.id && !selectedAgentId ? 'bg-blue-50 border-r-2 border-blue-500' : ''
                  }`}
                >
                  <button
                    onClick={() => toggleTaskExpansion(task.id)}
                    className="mr-2 p-0.5 hover:bg-gray-200 rounded"
                  >
                    {expandedTasks.has(task.id) ? (
                      <ChevronDownIcon className="w-4 h-4" />
                    ) : (
                      <ChevronRightIcon className="w-4 h-4" />
                    )}
                  </button>
                  <div 
                    className="flex-1 min-w-0"
                    onClick={() => onTaskSelect(task.id)}
                  >
                    <p className="font-medium text-gray-900 truncate">{task.name}</p>
                    <p className="text-sm text-gray-500 truncate">{task.prompt}</p>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onDeleteTask(task.id);
                    }}
                    className="ml-2 p-1 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded"
                  >
                    <TrashIcon className="w-4 h-4" />
                  </button>
                </div>

                {/* Agents */}
                {expandedTasks.has(task.id) && (
                  <div className="ml-8 border-l border-gray-200">
                    {task.agents.map((agent) => (
                      <div
                        key={agent.id}
                        className={`flex items-center px-4 py-2 hover:bg-gray-50 cursor-pointer ${
                          selectedAgentId === agent.id ? 'bg-blue-50 border-r-2 border-blue-500' : ''
                        }`}
                        onClick={() => onAgentSelect(task.id, agent.id)}
                      >
                        {getAgentStatusIcon(agent.status)}
                        <div className="ml-3 flex-1 min-w-0">
                          <p className="font-medium text-gray-700 truncate">{agent.name}</p>
                          <p className="text-sm text-gray-500">{getAgentStatusText(agent.status)}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Create task modal */}
      {showCreateModal && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4">
            <div className="p-6">
              <h3 className="text-lg font-medium text-gray-900 mb-4">Create New Task</h3>
              <textarea
                value={newTaskPrompt}
                onChange={(e) => setNewTaskPrompt(e.target.value)}
                placeholder="Enter your task prompt here..."
                className="w-full h-32 p-3 border border-gray-300 rounded-md resize-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                autoFocus
              />
              <div className="flex justify-end space-x-3 mt-4">
                <button
                  onClick={() => setShowCreateModal(false)}
                  className="px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 rounded-md transition-colors"
                  disabled={creatingTask}
                >
                  Cancel
                </button>
                <button
                  onClick={handleCreateTask}
                  disabled={!newTaskPrompt.trim() || creatingTask}
                  className="px-4 py-2 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors"
                >
                  {creatingTask ? 'Creating...' : 'Create Task'}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
