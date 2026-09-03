using UAPS.SDK.Client;
using UAPS.SDK.Models;
using UAPS.SDK.Analytics;

namespace UAPS.Workbench.Services;

/// <summary>
/// Global application state for the workbench with Undo/Redo support
/// </summary>
public class WorkbenchState
{
    /// <summary>
    /// Current schedule request being edited
    /// </summary>
    public ScheduleRequest CurrentRequest { get; set; } = new();

    /// <summary>
    /// Current scheduling result
    /// </summary>
    public ScheduleResult? CurrentResult { get; set; }

    /// <summary>
    /// KPI dashboard for current result
    /// </summary>
    public KpiDashboard? CurrentKpis { get; set; }

    /// <summary>
    /// Dispatching rule used for current result
    /// </summary>
    public string? CurrentRule { get; set; }

    /// <summary>
    /// Previous scheduling result (for comparison)
    /// </summary>
    public ScheduleResult? PreviousResult { get; set; }

    /// <summary>
    /// KPI dashboard for previous result
    /// </summary>
    public KpiDashboard? PreviousKpis { get; set; }

    /// <summary>
    /// Dispatching rule used for previous result
    /// </summary>
    public string? PreviousRule { get; set; }

    /// <summary>
    /// Whether we have comparison data
    /// </summary>
    public bool HasComparison => PreviousResult?.Schedule != null && CurrentResult?.Schedule != null;

    /// <summary>
    /// Currently selected dispatching configuration
    /// </summary>
    public DispatchingConfig DispatchingConfig { get; set; } = new()
    {
        PrimaryRule = DispatchingRules.FIFO
    };

    /// <summary>
    /// Path of the currently loaded file
    /// </summary>
    public string? CurrentFilePath { get; set; }

    /// <summary>
    /// Whether there are unsaved changes
    /// </summary>
    public bool HasUnsavedChanges { get; set; }

    #region Selection Sync

    /// <summary>
    /// Currently selected operation ID (synced across Table and Gantt)
    /// </summary>
    public string? SelectedOperationId { get; private set; }

    /// <summary>
    /// Event raised when selection changes
    /// </summary>
    public event Action<string?>? OnSelectionChanged;

    /// <summary>
    /// Select an operation (syncs across all views)
    /// </summary>
    public void SelectOperation(string? operationId)
    {
        if (SelectedOperationId != operationId)
        {
            SelectedOperationId = operationId;
            OnSelectionChanged?.Invoke(operationId);
        }
    }

    /// <summary>
    /// Clear selection
    /// </summary>
    public void ClearSelection()
    {
        SelectOperation(null);
    }

    #endregion

    #region Undo/Redo System

    private readonly List<UndoAction> _undoStack = [];
    private readonly List<UndoAction> _redoStack = [];
    private const int MaxHistorySize = 50;

    /// <summary>
    /// Event raised when undo/redo state changes
    /// </summary>
    public event Action? OnUndoRedoChanged;

    /// <summary>
    /// Whether undo is available
    /// </summary>
    public bool CanUndo => _undoStack.Count > 0;

    /// <summary>
    /// Whether redo is available
    /// </summary>
    public bool CanRedo => _redoStack.Count > 0;

    /// <summary>
    /// Get undo history (newest first)
    /// </summary>
    public IReadOnlyList<UndoAction> UndoHistory => _undoStack.AsReadOnly();

    /// <summary>
    /// Get redo history (oldest first)
    /// </summary>
    public IReadOnlyList<UndoAction> RedoHistory => _redoStack.AsReadOnly();

    /// <summary>
    /// Current position in history
    /// </summary>
    public int HistoryPosition => _undoStack.Count;

    /// <summary>
    /// Total history size
    /// </summary>
    public int TotalHistorySize => _undoStack.Count + _redoStack.Count;

    /// <summary>
    /// Record an action for undo
    /// </summary>
    public void RecordAction(string description, UndoActionType actionType, Action undoAction, Action redoAction)
    {
        // Clear redo stack when new action is recorded
        _redoStack.Clear();

        var action = new UndoAction
        {
            Description = description,
            ActionType = actionType,
            Timestamp = DateTime.Now,
            Undo = undoAction,
            Redo = redoAction
        };

        _undoStack.Add(action);

        // Limit history size
        while (_undoStack.Count > MaxHistorySize)
        {
            _undoStack.RemoveAt(0);
        }

        HasUnsavedChanges = true;
        OnUndoRedoChanged?.Invoke();
    }

    /// <summary>
    /// Undo the last action
    /// </summary>
    public UndoAction? Undo()
    {
        if (!CanUndo) return null;

        var action = _undoStack[^1];
        _undoStack.RemoveAt(_undoStack.Count - 1);

        action.Undo();

        _redoStack.Add(action);
        OnUndoRedoChanged?.Invoke();
        OnStateChanged?.Invoke();

        return action;
    }

    /// <summary>
    /// Redo the last undone action
    /// </summary>
    public UndoAction? Redo()
    {
        if (!CanRedo) return null;

        var action = _redoStack[^1];
        _redoStack.RemoveAt(_redoStack.Count - 1);

        action.Redo();

        _undoStack.Add(action);
        OnUndoRedoChanged?.Invoke();
        OnStateChanged?.Invoke();

        return action;
    }

    /// <summary>
    /// Jump to a specific point in history
    /// </summary>
    public void JumpToHistory(int position)
    {
        if (position < 0 || position > TotalHistorySize) return;

        while (HistoryPosition > position && CanUndo)
        {
            Undo();
        }

        while (HistoryPosition < position && CanRedo)
        {
            Redo();
        }
    }

    /// <summary>
    /// Clear all history
    /// </summary>
    public void ClearHistory()
    {
        _undoStack.Clear();
        _redoStack.Clear();
        OnUndoRedoChanged?.Invoke();
    }

    #endregion

    /// <summary>
    /// Event raised when state changes
    /// </summary>
    public event Action? OnStateChanged;

    /// <summary>
    /// Notify listeners that state has changed
    /// </summary>
    public void NotifyStateChanged()
    {
        HasUnsavedChanges = true;
        OnStateChanged?.Invoke();
    }

    /// <summary>
    /// Reset state to initial values
    /// </summary>
    public void Reset()
    {
        CurrentRequest = new ScheduleRequest();
        CurrentResult = null;
        CurrentKpis = null;
        CurrentRule = null;
        PreviousResult = null;
        PreviousKpis = null;
        PreviousRule = null;
        CurrentFilePath = null;
        HasUnsavedChanges = false;
        SelectedOperationId = null;
        ClearHistory();
        OnStateChanged?.Invoke();
    }

    /// <summary>
    /// Clear all simulation results
    /// </summary>
    public void ClearResults()
    {
        CurrentResult = null;
        CurrentKpis = null;
        CurrentRule = null;
        PreviousResult = null;
        PreviousKpis = null;
        PreviousRule = null;
    }
}

/// <summary>
/// Represents an undoable action
/// </summary>
public class UndoAction
{
    public string Description { get; set; } = string.Empty;
    public UndoActionType ActionType { get; set; }
    public DateTime Timestamp { get; set; }
    public Action Undo { get; set; } = () => { };
    public Action Redo { get; set; } = () => { };
}

/// <summary>
/// Type of undo action
/// </summary>
public enum UndoActionType
{
    Add,
    Edit,
    Delete,
    Move,
    Load
}
