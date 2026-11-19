use std::sync::Arc;

use gpui_kit::{
    App, AppContext as _, Context, ElementId, Entity, Focusable as _,
    IntoElement, ParentElement, Styled, Subscription, Window,
    component::{
        ActiveTheme as _, Selectable as _, Sizable as _, Size,
        button::Button,
        color_picker::{ColorPickerEvent, ColorPickerState, ColorSelect},
        input::{InputEvent, InputState, NumberInput},
        label::Label,
        slider::{Slider, SliderEvent, SliderState, SliderValue},
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px, rgb,
};

use crate::app::StudioApp;

pub trait ButtonSelectedExt {
    fn primary_selected(self, is_selected: bool, cx: &App) -> Self;
}

impl ButtonSelectedExt for Button {
    fn primary_selected(self, is_selected: bool, cx: &App) -> Self {
        self.selected(is_selected).when(is_selected, |b| {
            b.bg(cx.theme().primary)
                .text_color(cx.theme().primary_foreground)
                .border_color(cx.theme().border)
        })
    }
}

struct NumberFieldState {
    input: Entity<InputState>,
    suppress_value: Option<String>,
    _sub: Subscription,
}

pub(crate) fn render_number_field(
    key: impl Into<ElementId>,
    label_text: impl AsRef<str>,
    val: f32,
    step: f32,
    min: f32,
    max: f32,
    precision: usize,
    unit: &'static str,
    app: &Entity<StudioApp>,
    window: &mut Window,
    cx: &mut Context<super::InspectorView>,
    on_change: impl Fn(f32, &mut StudioApp, &mut Context<StudioApp>) + Send + Sync + 'static,
) -> impl IntoElement {
    let label_text = label_text.as_ref().to_string();
    let key = key.into();
    let app_for_sub = app.clone();
    let on_change = Arc::new(on_change);

    let field_state = window.use_keyed_state(key, cx, |window, cx| {
        let default_val_str = if precision == 0 {
            format!("{val:.0}")
        } else {
            format!("{val:.1}")
        };
        // Parse step/min/max via string formatting to prevent f32->f64 binary floating point imprecision
        // (e.g. 0.1f32 becoming 0.10000000149011612f64 which ruins NumberInput fraction digit calculation)
        let clean_step = format!("{step:.4}").trim_end_matches('0').trim_end_matches('.').parse::<f64>().unwrap_or(step as f64);
        let clean_min = format!("{min:.4}").trim_end_matches('0').trim_end_matches('.').parse::<f64>().unwrap_or(min as f64);
        let clean_max = format!("{max:.4}").trim_end_matches('0').trim_end_matches('.').parse::<f64>().unwrap_or(max as f64);

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(default_val_str)
                .step(clean_step)
                .min(clean_min)
                .max(clean_max)
        });

        let sub = cx.subscribe_in(
            &input,
            window,
            move |this: &mut NumberFieldState, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        let text = input.read(cx).value();
                        if this.suppress_value.as_deref() == Some(text.as_ref()) {
                            this.suppress_value = None;
                            return;
                        }
                        this.suppress_value = None;
                        if let Ok(parsed) = text.parse::<f32>()
                            && parsed >= min
                            && parsed <= max
                        {
                            let cb = on_change.clone();
                            app_for_sub.update(cx, |app, cx| {
                                cb(parsed, app, cx);
                            });
                        }
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        let text = input.read(cx).value();
                        if let Ok(parsed) = text.parse::<f32>() {
                            let clamped = parsed.clamp(min, max);
                            let formatted = if precision == 0 {
                                format!("{clamped:.0}")
                            } else {
                                format!("{clamped:.1}")
                            };
                            if text.as_ref() != formatted.as_str() {
                                this.suppress_value = Some(formatted.clone());
                                input.update(cx, |state, cx| {
                                    state.set_value(formatted, window, cx);
                                });
                            }
                            let cb = on_change.clone();
                            app_for_sub.update(cx, |app, cx| {
                                cb(clamped, app, cx);
                            });
                        }
                    }
                    InputEvent::Focus => {}
                }
            },
        );

        NumberFieldState {
            input,
            suppress_value: None,
            _sub: sub,
        }
    });

    let current_str = if precision == 0 {
        format!("{val:.0}")
    } else {
        format!("{val:.1}")
    };
    let input_entity = field_state.read(cx).input.clone();
    if input_entity.read(cx).value().as_ref() != current_str.as_str()
        && !input_entity.read(cx).focus_handle(cx).is_focused(window)
    {
        field_state.update(cx, |st, _| {
            st.suppress_value = Some(current_str.clone());
        });
        input_entity.update(cx, |state, cx| {
            state.set_value(current_str, window, cx);
        });
    }

    v_flex()
        .gap(px(4.0))
        .w_full()
        .child(
            Label::new(format!("{label_text} ({unit})"))
                .text_sm()
                .text_color(cx.theme().muted_foreground),
        )
        .child(NumberInput::new(&input_entity).w_full())
}

struct RoundedSliderState {
    slider: Entity<SliderState>,
    _sub: Subscription,
}

pub(crate) fn render_rounded_slider(
    item_id: u64,
    current_radius: f32,
    app: &Entity<StudioApp>,
    window: &mut Window,
    cx: &mut Context<super::InspectorView>,
) -> impl IntoElement {
    let app_for_sub = app.clone();
    let slider_key = ("item-rounded-slider", item_id);

    let slider_state = window.use_keyed_state(slider_key, cx, |window, cx| {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(200.0)
                .step(1.0)
                .default_value(SliderValue::Single(current_radius))
        });

        let sub = cx.subscribe_in(
            &slider,
            window,
            move |_this: &mut RoundedSliderState,
                  _slider,
                  event: &SliderEvent,
                  _window,
                  cx| {
                if let SliderEvent::Change(SliderValue::Single(val)) = event {
                    let r = *val;
                    app_for_sub.update(cx, |app, cx| {
                        if let Some(item) = app.state.find_item_mut(item_id) {
                            item.corner_radius = r;
                            cx.notify();
                        }
                    });
                }
            },
        );

        RoundedSliderState { slider, _sub: sub }
    });

    let slider_entity = slider_state.read(cx).slider.clone();
    let current_slider_val = match slider_entity.read(cx).value() {
        SliderValue::Single(v) => v,
        _ => 0.0,
    };
    if (current_slider_val - current_radius).abs() > 0.5 {
        slider_entity.update(cx, |state, cx| {
            state.set_value(SliderValue::Single(current_radius), window, cx);
        });
    }

    Slider::new(&slider_entity).w_full()
}

struct ColorPickerHostState {
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

pub(crate) fn render_color_select(
    key: impl Into<ElementId>,
    current_color: Option<gpui_kit::Hsla>,
    default_color: gpui_kit::Hsla,
    app: &Entity<StudioApp>,
    window: &mut Window,
    cx: &mut Context<super::InspectorView>,
    on_change: impl Fn(Option<gpui_kit::Hsla>, &mut StudioApp, &mut Context<StudioApp>)
        + Send
        + Sync
        + 'static,
) -> impl IntoElement {
    let app_for_sub = app.clone();
    let initial_color = current_color.unwrap_or(default_color);
    let on_change = Arc::new(on_change);

    let host_state = window.use_keyed_state(key.into(), cx, |window, cx| {
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial_color));

        let sub = cx.subscribe_in(
            &picker,
            window,
            move |_this: &mut ColorPickerHostState,
                  _picker,
                  event: &ColorPickerEvent,
                  _window,
                  cx| {
                let ColorPickerEvent::Change(color) = event;
                let color = *color;
                let cb = on_change.clone();
                app_for_sub.update(cx, |app, cx| {
                    cb(color, app, cx);
                });
            },
        );

        ColorPickerHostState {
            picker,
            _sub: sub,
        }
    });

    let picker_entity = host_state.read(cx).picker.clone();
    if picker_entity.read(cx).value() != current_color {
        if let Some(col) = current_color {
            picker_entity.update(cx, |state, cx| {
                state.set_value(col, window, cx);
            });
        }
    }

    div()
        .w_full()
        .child(ColorSelect::new(&picker_entity).with_size(Size::Medium).w_full())
}

pub(crate) fn render_bg_color_select(
    item_id: u64,
    current_bg: Option<gpui_kit::Hsla>,
    app: &Entity<StudioApp>,
    window: &mut Window,
    cx: &mut Context<super::InspectorView>,
) -> impl IntoElement {
    render_color_select(
        ("item-bg-color-picker", item_id),
        current_bg,
        rgb(0xDC2626).into(),
        app,
        window,
        cx,
        move |color, app, cx| {
            if let Some(item) = app.state.find_item_mut(item_id) {
                item.bg_color = color;
                cx.notify();
            }
        },
    )
}
