'use client';

import { useState, useEffect, useCallback } from 'react';
import { Task, CreateTaskRequest, CreateTaskResponse } from '@/types';

export function useTasks() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch tasks from API
  const fetchTasks = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    
    try {
      const response = await fetch('/api/tasks');
      if (!response.ok) {
        throw new Error(`Failed to fetch tasks: ${response.statusText}`);
      }
      
      const tasksData: Task[] = await response.json();
      setTasks(tasksData);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      console.error('Error fetching tasks:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Create a new task
  const createTask = useCallback(async (request: CreateTaskRequest) => {
    setError(null);
    
    try {
      const response = await fetch('/api/tasks', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(request),
      });
      
      if (!response.ok) {
        throw new Error(`Failed to create task: ${response.statusText}`);
      }
      
      const result: CreateTaskResponse = await response.json();
      
      // Add the new task to the current list
      setTasks(prevTasks => [result.task, ...prevTasks]);
      
      return result.task;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      console.error('Error creating task:', err);
      throw err;
    }
  }, []);

  // Delete a task
  const deleteTask = useCallback(async (taskId: string) => {
    setError(null);
    
    try {
      const response = await fetch(`/api/tasks/${taskId}`, {
        method: 'DELETE',
      });
      
      if (!response.ok) {
        throw new Error(`Failed to delete task: ${response.statusText}`);
      }
      
      // Remove the task from the current list
      setTasks(prevTasks => prevTasks.filter(task => task.id !== taskId));
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      console.error('Error deleting task:', err);
      throw err;
    }
  }, []);

  // Refresh tasks periodically
  useEffect(() => {
    fetchTasks();
    
    // Set up polling to refresh tasks every 5 seconds
    const interval = setInterval(fetchTasks, 5000);
    
    return () => clearInterval(interval);
  }, [fetchTasks]);

  return {
    tasks,
    isLoading,
    error,
    createTask,
    deleteTask,
    refetch: fetchTasks,
  };
}
