import { box, onMountEffect, useRefById } from "svelte-toolbelt";
import { on } from "svelte/events";
import { Context, watch } from "runed";
import { useTimeoutFn } from "../../internal/use-timeout-fn.svelte.js";
import { isElement, isFocusVisible } from "../../internal/is.js";
import { useGraceArea } from "../../internal/use-grace-area.svelte.js";
import { getDataDisabled } from "../../internal/attrs.js";
const TOOLTIP_CONTENT_ATTR = "data-tooltip-content";
const TOOLTIP_TRIGGER_ATTR = "data-tooltip-trigger";
class TooltipProviderState {
    opts;
    isOpenDelayed = $state(true);
    isPointerInTransit = box(false);
    #timerFn;
    #openTooltip = $state(null);
    constructor(opts) {
        this.opts = opts;
        this.#timerFn = useTimeoutFn(() => {
            this.isOpenDelayed = true;
        }, this.opts.skipDelayDuration.current, { immediate: false });
    }
    #startTimer = () => {
        const skipDuration = this.opts.skipDelayDuration.current;
        if (skipDuration === 0) {
            return;
        }
        else {
            this.#timerFn.start();
        }
    };
    #clearTimer = () => {
        this.#timerFn.stop();
    };
    onOpen = (tooltip) => {
        if (this.#openTooltip && this.#openTooltip !== tooltip) {
            this.#openTooltip.handleClose();
        }
        this.#clearTimer();
        this.isOpenDelayed = false;
        this.#openTooltip = tooltip;
    };
    onClose = (tooltip) => {
        if (this.#openTooltip === tooltip) {
            this.#openTooltip = null;
        }
        this.#startTimer();
    };
    isTooltipOpen = (tooltip) => {
        return this.#openTooltip === tooltip;
    };
}
class TooltipRootState {
    opts;
    provider;
    delayDuration = $derived.by(() => this.opts.delayDuration.current ?? this.provider.opts.delayDuration.current);
    disableHoverableContent = $derived.by(() => this.opts.disableHoverableContent.current ??
        this.provider.opts.disableHoverableContent.current);
    disableCloseOnTriggerClick = $derived.by(() => this.opts.disableCloseOnTriggerClick.current ??
        this.provider.opts.disableCloseOnTriggerClick.current);
    disabled = $derived.by(() => this.opts.disabled.current ?? this.provider.opts.disabled.current);
    ignoreNonKeyboardFocus = $derived.by(() => this.opts.ignoreNonKeyboardFocus.current ??
        this.provider.opts.ignoreNonKeyboardFocus.current);
    contentNode = $state(null);
    triggerNode = $state(null);
    #wasOpenDelayed = $state(false);
    #timerFn;
    stateAttr = $derived.by(() => {
        if (!this.opts.open.current)
            return "closed";
        return this.#wasOpenDelayed ? "delayed-open" : "instant-open";
    });
    constructor(opts, provider) {
        this.opts = opts;
        this.provider = provider;
        this.#timerFn = useTimeoutFn(() => {
            this.#wasOpenDelayed = true;
            this.opts.open.current = true;
        }, this.delayDuration ?? 0, { immediate: false });
        watch(() => this.delayDuration, () => {
            if (this.delayDuration === undefined)
                return;
            this.#timerFn = useTimeoutFn(() => {
                this.#wasOpenDelayed = true;
                this.opts.open.current = true;
            }, this.delayDuration, { immediate: false });
        });
        watch(() => this.opts.open.current, (isOpen) => {
            if (isOpen) {
                this.provider.onOpen(this);
            }
            else {
                this.provider.onClose(this);
            }
        });
    }
    handleOpen = () => {
        this.#timerFn.stop();
        this.#wasOpenDelayed = false;
        this.opts.open.current = true;
    };
    handleClose = () => {
        this.#timerFn.stop();
        this.opts.open.current = false;
    };
    #handleDelayedOpen = () => {
        this.#timerFn.stop();
        const shouldSkipDelay = !this.provider.isOpenDelayed;
        const delayDuration = this.delayDuration ?? 0;
        // if no delay needed (either skip delay active or delay is 0), open immediately
        if (shouldSkipDelay || delayDuration === 0) {
            // set wasOpenDelayed based on whether we actually had a delay
            this.#wasOpenDelayed = delayDuration > 0 && shouldSkipDelay;
            this.opts.open.current = true;
        }
        else {
            // use timer for actual delays
            this.#timerFn.start();
        }
    };
    onTriggerEnter = () => {
        this.#handleDelayedOpen();
    };
    onTriggerLeave = () => {
        if (this.disableHoverableContent) {
            this.handleClose();
        }
        else {
            this.#timerFn.stop();
        }
    };
}
class TooltipTriggerState {
    opts;
    root;
    #isPointerDown = box(false);
    #hasPointerMoveOpened = $state(false);
    #isDisabled = $derived.by(() => this.opts.disabled.current || this.root.disabled);
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        useRefById({
            ...opts,
            onRefChange: (node) => {
                this.root.triggerNode = node;
            },
        });
    }
    handlePointerUp = () => {
        this.#isPointerDown.current = false;
    };
    #onpointerup = () => {
        if (this.#isDisabled)
            return;
        this.#isPointerDown.current = false;
    };
    #onpointerdown = () => {
        if (this.#isDisabled)
            return;
        this.#isPointerDown.current = true;
        document.addEventListener("pointerup", () => {
            this.handlePointerUp();
        }, { once: true });
    };
    #onpointermove = (e) => {
        if (this.#isDisabled)
            return;
        if (e.pointerType === "touch")
            return;
        if (this.#hasPointerMoveOpened)
            return;
        if (this.root.provider.isPointerInTransit.current)
            return;
        this.root.onTriggerEnter();
        this.#hasPointerMoveOpened = true;
    };
    #onpointerleave = () => {
        if (this.#isDisabled)
            return;
        this.root.onTriggerLeave();
        this.#hasPointerMoveOpened = false;
    };
    #onfocus = (e) => {
        if (this.#isPointerDown.current || this.#isDisabled)
            return;
        if (this.root.ignoreNonKeyboardFocus && !isFocusVisible(e.currentTarget))
            return;
        this.root.handleOpen();
    };
    #onblur = () => {
        if (this.#isDisabled)
            return;
        this.root.handleClose();
    };
    #onclick = () => {
        if (this.root.disableCloseOnTriggerClick || this.#isDisabled)
            return;
        this.root.handleClose();
    };
    props = $derived.by(() => ({
        id: this.opts.id.current,
        "aria-describedby": this.root.opts.open.current ? this.root.contentNode?.id : undefined,
        "data-state": this.root.stateAttr,
        "data-disabled": getDataDisabled(this.#isDisabled),
        "data-delay-duration": `${this.root.delayDuration}`,
        [TOOLTIP_TRIGGER_ATTR]: "",
        tabindex: this.#isDisabled ? undefined : 0,
        disabled: this.opts.disabled.current,
        onpointerup: this.#onpointerup,
        onpointerdown: this.#onpointerdown,
        onpointermove: this.#onpointermove,
        onpointerleave: this.#onpointerleave,
        onfocus: this.#onfocus,
        onblur: this.#onblur,
        onclick: this.#onclick,
    }));
}
class TooltipContentState {
    opts;
    root;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        useRefById({
            ...opts,
            onRefChange: (node) => {
                this.root.contentNode = node;
            },
            deps: () => this.root.opts.open.current,
        });
        useGraceArea({
            triggerNode: () => this.root.triggerNode,
            contentNode: () => this.root.contentNode,
            enabled: () => this.root.opts.open.current && !this.root.disableHoverableContent,
            onPointerExit: () => {
                if (this.root.provider.isTooltipOpen(this.root)) {
                    this.root.handleClose();
                }
            },
            setIsPointerInTransit: (value) => {
                this.root.provider.isPointerInTransit.current = value;
            },
            transitTimeout: this.root.provider.opts.skipDelayDuration.current,
        });
        onMountEffect(() => on(window, "scroll", (e) => {
            const target = e.target;
            if (!target)
                return;
            if (target.contains(this.root.triggerNode)) {
                this.root.handleClose();
            }
        }));
    }
    onInteractOutside = (e) => {
        if (isElement(e.target) &&
            this.root.triggerNode?.contains(e.target) &&
            this.root.disableCloseOnTriggerClick) {
            e.preventDefault();
            return;
        }
        this.opts.onInteractOutside.current(e);
        if (e.defaultPrevented)
            return;
        this.root.handleClose();
    };
    onEscapeKeydown = (e) => {
        this.opts.onEscapeKeydown.current?.(e);
        if (e.defaultPrevented)
            return;
        this.root.handleClose();
    };
    onOpenAutoFocus = (e) => {
        e.preventDefault();
    };
    onCloseAutoFocus = (e) => {
        e.preventDefault();
    };
    snippetProps = $derived.by(() => ({ open: this.root.opts.open.current }));
    props = $derived.by(() => ({
        id: this.opts.id.current,
        "data-state": this.root.stateAttr,
        "data-disabled": getDataDisabled(this.root.disabled),
        style: {
            pointerEvents: "auto",
            outline: "none",
        },
        [TOOLTIP_CONTENT_ATTR]: "",
    }));
    popperProps = {
        onInteractOutside: this.onInteractOutside,
        onEscapeKeydown: this.onEscapeKeydown,
        onOpenAutoFocus: this.onOpenAutoFocus,
        onCloseAutoFocus: this.onCloseAutoFocus,
    };
}
//
// CONTEXT METHODS
//
const TooltipProviderContext = new Context("Tooltip.Provider");
const TooltipRootContext = new Context("Tooltip.Root");
export function useTooltipProvider(props) {
    return TooltipProviderContext.set(new TooltipProviderState(props));
}
export function useTooltipRoot(props) {
    return TooltipRootContext.set(new TooltipRootState(props, TooltipProviderContext.get()));
}
export function useTooltipTrigger(props) {
    return new TooltipTriggerState(props, TooltipRootContext.get());
}
export function useTooltipContent(props) {
    return new TooltipContentState(props, TooltipRootContext.get());
}
