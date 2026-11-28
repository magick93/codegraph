CREATE OR REPLACE FUNCTION public.role_at_least(p_min_role text)
 RETURNS boolean LANGUAGE plpgsql STABLE SECURITY DEFINER
AS $function$
DECLARE
    v_role TEXT;
BEGIN
    v_role := coalesce(current_setting('app.role', true), '');
    -- api_key_mode = "agent" (issue #635): API-key sessions pass by SESSION
    -- KIND (`app.session_kind`, set by the request bundle), never by the
    -- `app.role` literal. The actor policy gates them in the handler.
    IF v_role = '' OR coalesce(current_setting('app.session_kind', true), '') = 'agent' THEN
        RETURN TRUE;
    END IF;
    IF public.role_rank(v_role) <= public.role_rank(p_min_role) THEN
        RETURN TRUE;
    END IF;
    RAISE EXCEPTION USING
        ERRCODE = 'P0403',
        MESSAGE = json_build_object('code', 'ROLE_FORBIDDEN', 'role', v_role, 'required', p_min_role)::text;
END;
$function$;
